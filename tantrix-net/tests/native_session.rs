#![cfg(all(feature = "native", not(target_arch = "wasm32")))]

use std::time::Duration;

use futures::StreamExt;
use tantrix_game::Color::{Blue, Red};
use tantrix_net::{open_native_session, SessionCommand, SessionEvent, SessionHandle};

async fn next_event(session: &mut SessionHandle) -> SessionEvent {
    tokio::time::timeout(Duration::from_secs(10), session.events.next())
        .await
        .expect("server did not respond")
        .expect("session ended")
}

#[tokio::test]
async fn native_handshake_sends_cookie_and_commands_and_closes_on_leave() {
    use futures::SinkExt;
    use tantrix_proto::{decode, encode, HostMessage, LobbyState, PeerId, PeerMessage};
    use tokio_tungstenite::{
        accept_hdr_async,
        tungstenite::{
            handshake::server::{Request, Response},
            Message,
        },
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(stream, |request: &Request, response: Response| {
            assert_eq!(request.uri(), "/room/ABCDEF?create=1");
            assert_eq!(
                request.headers()["Cookie"],
                "tantrix_session=0123456789abcdef"
            );
            assert!(!request.headers().contains_key("Origin"));
            Ok(response)
        })
        .await
        .unwrap();
        let hello = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(
            matches!(decode::<PeerMessage>(hello.as_bytes()).unwrap(), PeerMessage::Hello { nickname } if nickname == "Desktop")
        );
        let welcome = HostMessage::Welcome {
            your_peer_id: PeerId::new("desktop"),
            lobby: LobbyState {
                seats: Default::default(),
                unseated: vec![],
                participants: vec![],
            },
            rejected: None,
        };
        socket
            .send(Message::Text(
                String::from_utf8(encode(&welcome)).unwrap().into(),
            ))
            .await
            .unwrap();
        let command = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(matches!(
            decode::<PeerMessage>(command.as_bytes()).unwrap(),
            PeerMessage::PickColor { color: Some(Red) }
        ));
        assert!(matches!(
            socket.next().await.unwrap().unwrap(),
            Message::Close(_)
        ));
    });
    let (mut session, run) = open_native_session(
        format!("ws://{address}/room/ABCDEF?create=1"),
        "Desktop".into(),
        "tantrix_session=0123456789abcdef".into(),
    );
    let client = tokio::spawn(run);
    assert!(matches!(
        next_event(&mut session).await,
        SessionEvent::Connected { .. }
    ));
    assert!(matches!(
        next_event(&mut session).await,
        SessionEvent::Lobby(_)
    ));
    session
        .commands
        .unbounded_send(SessionCommand::PickColor(Some(Red)))
        .unwrap();
    session
        .commands
        .unbounded_send(SessionCommand::Leave)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    client.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_server_that_never_finishes_the_upgrade_times_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (mut session, run) = open_native_session(
        format!("ws://{address}/room/ABCDEF"),
        "Desktop".into(),
        "tantrix_session=0123456789abcdef".into(),
    );
    let client = tokio::spawn(run);
    let (_stream, _) = listener.accept().await.unwrap();
    tokio::time::advance(Duration::from_secs(21)).await;
    assert!(matches!(
        session.events.next().await,
        Some(SessionEvent::ConnectTimeout)
    ));
    client.await.unwrap();
}

/// Run against either real backend with TANTRIX_TEST_SERVER=https://host.
#[tokio::test]
#[ignore = "requires a running Tantrix room server"]
async fn two_native_players_play_reconnect_and_end_a_game() {
    let server = std::env::var("TANTRIX_TEST_SERVER").expect("set TANTRIX_TEST_SERVER");
    let code = tantrix_net::generate_room_code();
    let base = format!(
        "{}/room/{code}",
        server.trim_end_matches('/').replacen("http", "ws", 1)
    );
    let host_cookie = format!("tantrix_session=desktop-host-{code}");
    let guest_cookie = format!("tantrix_session=desktop-guest-{code}");
    let (mut host, host_run) = open_native_session(
        format!("{base}?create=1"),
        "Desktop host".into(),
        host_cookie.clone(),
    );
    let host_task = tokio::spawn(host_run);
    let host_id = match next_event(&mut host).await {
        SessionEvent::Connected { my_peer_id } => my_peer_id,
        event => panic!("unexpected event: {event:?}"),
    };
    assert!(matches!(
        next_event(&mut host).await,
        SessionEvent::Lobby(_)
    ));
    host.commands
        .unbounded_send(SessionCommand::PickColor(Some(Red)))
        .unwrap();
    while let SessionEvent::Lobby(lobby) = next_event(&mut host).await {
        if lobby.seats[Red]
            .as_ref()
            .is_some_and(|seat| seat.peer_id == host_id)
        {
            break;
        }
    }
    let (mut guest, guest_run) =
        open_native_session(base.clone(), "Desktop guest".into(), guest_cookie);
    let guest_task = tokio::spawn(guest_run);
    assert!(matches!(
        next_event(&mut guest).await,
        SessionEvent::Connected { .. }
    ));
    assert!(matches!(
        next_event(&mut guest).await,
        SessionEvent::Lobby(_)
    ));
    guest
        .commands
        .unbounded_send(SessionCommand::PickColor(Some(Blue)))
        .unwrap();
    loop {
        match next_event(&mut guest).await {
            SessionEvent::Lobby(lobby) if lobby.seats[Blue].is_some() => break,
            SessionEvent::Lobby(_) => {}
            event => panic!("unexpected event: {event:?}"),
        }
    }
    host.commands
        .unbounded_send(SessionCommand::StartGame)
        .unwrap();
    let snapshot = loop {
        match next_event(&mut host).await {
            SessionEvent::GameStarted { snapshot } => break snapshot,
            SessionEvent::Lobby(_) => {}
            event => panic!("unexpected event: {event:?}"),
        }
    };
    let placement = snapshot.board.possible_moves(
        &snapshot.hands[snapshot.current_player],
        snapshot.bag_count == 0,
        snapshot.turn_step,
    )[0]
    .placement();
    let player = if snapshot.current_player == Red {
        &host
    } else {
        &guest
    };
    player
        .commands
        .unbounded_send(SessionCommand::PlayMove(placement))
        .unwrap();
    for session in [&mut host, &mut guest] {
        loop {
            match next_event(session).await {
                SessionEvent::StateUpdate { snapshot, .. } if snapshot.board.len() == 1 => break,
                SessionEvent::Lobby(_) | SessionEvent::GameStarted { .. } => {}
                event => panic!("unexpected event: {event:?}"),
            }
        }
    }
    host.commands.unbounded_send(SessionCommand::Leave).unwrap();
    host_task.await.unwrap();
    let (mut host, run) = open_native_session(base, "Returning desktop".into(), host_cookie);
    let host_task = tokio::spawn(run);
    assert!(
        matches!(next_event(&mut host).await, SessionEvent::Connected { my_peer_id } if my_peer_id == host_id)
    );
    loop {
        match next_event(&mut host).await {
            SessionEvent::GameStarted { snapshot } | SessionEvent::StateUpdate { snapshot, .. } => {
                assert_eq!(snapshot.board.len(), 1);
                assert_eq!(snapshot.seats[Red].as_ref().unwrap().peer_id, host_id);
                break;
            }
            SessionEvent::Lobby(_) => {}
            event => panic!("unexpected event: {event:?}"),
        }
    }
    host.commands
        .unbounded_send(SessionCommand::EndGame)
        .unwrap();
    for session in [&mut host, &mut guest] {
        loop {
            match next_event(session).await {
                SessionEvent::GameEnded { .. } => break,
                SessionEvent::StateUpdate { .. } => {}
                event => panic!("unexpected event: {event:?}"),
            }
        }
        session
            .commands
            .unbounded_send(SessionCommand::Leave)
            .unwrap();
    }
    host_task.await.unwrap();
    guest_task.await.unwrap();
}
