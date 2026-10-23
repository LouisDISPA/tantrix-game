//! Fullstack server entrypoint.
//!
//! Serves the Dioxus app and native online rooms, with **server-side rendering
//! and hydration** through Dioxus's Axum handlers. Known application routes
//! render on the server; unknown paths return the shared static 404 page.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ws::WebSocketUpgrade, Extension, Path, State};
use axum::http::{header, HeaderValue, Request, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use dioxus::server::{DioxusRouterExt, FullstackState, ServeConfig};
use tantrix_proto::{is_game_id, CreateRoom, RoomApiError};
use tantrix_room::handshake::{self, Refusal};
use tantrix_room::lifecycle;
use tantrix_room::session_cookie::{self, SessionToken};
#[cfg(not(debug_assertions))]
use tower_http::compression::CompressionLayer;

use crate::app;
use crate::route::Route;

mod room;
mod store;

struct ServerState {
    rooms: room::RoomManager,
    public_origin: Option<String>,
}

/// Where to bind the HTTP listener.
///
/// Resolution order:
/// 1. `BIND_ADDR=host:port`      — explicit override.
/// 2. `IP` + `PORT`              — Dioxus CLI convention (`dx serve` sets
///    these to a private internal port so dx can run its devserver proxy
///    on the user-facing port and intercept `/_dioxus/*` for hot reload).
///    Also Fly/Heroku/Render/etc.
/// 3. Fallback: `0.0.0.0:8080`.
///
/// Honoring `IP`/`PORT` is what stops the wasm bundle from spending its
/// retry budget on `ws://localhost:PORT/_dioxus` 404s in dev — when our
/// server is on the internal port, dx's proxy serves `/_dioxus` directly.
fn bind_addr() -> SocketAddr {
    if let Ok(addr) = std::env::var("BIND_ADDR") {
        return addr.parse().expect("BIND_ADDR must be host:port");
    }
    let ip: std::net::IpAddr = std::env::var("IP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    SocketAddr::new(ip, port)
}

pub fn run() {
    setup_logging();
    let addr = bind_addr();
    println!("Tantrix fullstack server listening on {addr}");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    runtime.block_on(async move {
        let db_path =
            std::env::var("TANTRIX_ROOM_DB").unwrap_or_else(|_| "tantrix-rooms.sqlite3".into());
        let store = store::RoomStore::open(&db_path).expect("failed to open room database");
        let state = Arc::new(ServerState {
            rooms: room::RoomManager::new(store),
            public_origin: std::env::var("TANTRIX_PUBLIC_ORIGIN")
                .ok()
                .map(|origin| origin.trim_end_matches('/').to_owned()),
        });
        let cleanup_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Err(error) = cleanup_state.rooms.cleanup(room::now_ms()).await {
                    log::error!("room cleanup failed: {error}");
                }
            }
        });
        let router = Router::new()
            .route("/room/{id}", get(room_socket))
            .route("/api/rooms", post(create_room))
            .route("/api/rooms/code/{code}", get(resolve_room))
            .route("/api/rooms/{id}", get(room_reference))
            .route("/health", get(|| async { "OK" }))
            .route(
                tantrix_proto::ONLINE_VERSION_PATH,
                get(|| async {
                    (
                        [("cache-control", "no-store")],
                        axum::Json(tantrix_proto::OnlineVersion::CURRENT),
                    )
                }),
            )
            .with_state(state.clone())
            .merge(
                Router::new()
                    .register_server_functions()
                    .serve_static_assets()
                    .fallback(get(render_page))
                    .with_state(FullstackState::new(ServeConfig::new(), app)),
            )
            .layer(from_fn_with_state(state, session_cookie_layer));

        #[cfg(not(debug_assertions))]
        let router = router.layer(CompressionLayer::new());

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("failed to bind fullstack server");
        axum::serve(listener, router)
            .await
            .expect("fullstack server exited with an error");
    });
}

async fn render_page(state: State<FullstackState>, request: Request<axum::body::Body>) -> Response {
    // Reuse the application's route parser rather than maintaining a second
    // allowlist. Static assets and server endpoints are matched before this.
    match request.uri().path().parse::<Route>() {
        Ok(Route::NotFound { .. }) | Err(_) => (
            StatusCode::NOT_FOUND,
            Html(include_str!("../public/404.html")),
        )
            .into_response(),
        Ok(_) => FullstackState::render_handler(state, request).await,
    }
}

async fn session_cookie_layer(
    State(state): State<Arc<ServerState>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok());
    let secure = state
        .public_origin
        .as_deref()
        .is_some_and(|origin| origin.starts_with("https://"));
    let (token, set_cookie) = session_cookie::identify(cookie, secure);
    request.extensions_mut().insert(token);
    let mut response = next.run(request).await;
    if let Some(value) = set_cookie.and_then(|value| HeaderValue::from_str(&value).ok()) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

fn api_error(error: RoomApiError) -> Response {
    (
        StatusCode::from_u16(error.status()).unwrap(),
        [("cache-control", "no-store")],
        Json(error),
    )
        .into_response()
}

fn api_result(result: Result<tantrix_proto::RoomReference, RoomApiError>) -> Response {
    match result {
        Ok(reference) => ([("cache-control", "no-store")], Json(reference)).into_response(),
        Err(error) => api_error(error),
    }
}

async fn create_room(
    State(state): State<Arc<ServerState>>,
    Extension(token): Extension<SessionToken>,
    Json(request): Json<CreateRoom>,
) -> Response {
    if !is_game_id(&request.request_id) {
        return api_error(RoomApiError::InvalidRequest);
    }
    let id = lifecycle::game_id(&token, &request.request_id);
    api_result(state.rooms.create(&id, room::now_ms()).await)
}

async fn resolve_room(State(state): State<Arc<ServerState>>, Path(code): Path<String>) -> Response {
    if !tantrix_proto::is_room_code(&code) {
        return api_error(RoomApiError::InvalidRequest);
    }
    api_result(state.rooms.resolve(&code, room::now_ms()).await)
}

async fn room_reference(State(state): State<Arc<ServerState>>, Path(id): Path<String>) -> Response {
    if !is_game_id(&id) {
        return api_error(RoomApiError::InvalidRequest);
    }
    api_result(state.rooms.reference(&id, room::now_ms(), true).await)
}

async fn room_socket(
    State(state): State<Arc<ServerState>>,
    Path(code): Path<String>,
    Extension(token): Extension<SessionToken>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if let Err(refusal) = handshake::check(&code) {
        return refuse(refusal);
    }

    match state.rooms.open(&code).await {
        Ok(room::OpenRoom::Ready(handle)) => match handle.inspect(room::now_ms(), true).await {
            Ok(_) => upgrade.on_upgrade(move |socket| room::socket_session(socket, handle, token)),
            Err(RoomApiError::Expired) => upgrade.on_upgrade(|socket| {
                room::rejected_room(socket, tantrix_proto::RejectCode::RoomExpired)
            }),
            Err(error) => api_error(error),
        },
        Ok(room::OpenRoom::Missing) => upgrade.on_upgrade(room::missing_room),
        Ok(room::OpenRoom::Expired) => upgrade.on_upgrade(|socket| {
            room::rejected_room(socket, tantrix_proto::RejectCode::RoomExpired)
        }),
        Err(error) => {
            log::error!("failed to open room {code}: {error}");
            refuse(Refusal::StorageUnavailable)
        }
    }
}

fn refuse(refusal: Refusal) -> Response {
    let status =
        StatusCode::from_u16(refusal.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, refusal.message()).into_response()
}

fn setup_logging() {
    use tracing_subscriber::prelude::*;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "tantrix_app=info,tower_http=info".into());
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().compact())
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use dioxus::prelude::*;

    #[tokio::test]
    async fn unknown_pages_skip_ssr_and_known_pages_still_render() {
        fn test_app() -> Element {
            rsx! { p { "SSR route reached" } }
        }

        let state = FullstackState::new(ServeConfig::new(), test_app);
        for path in ["/firebase-key.json", "/www/phpinfo.php", "/missing?query=1"] {
            let response = render_page(
                State(state.clone()),
                Request::builder().uri(path).body(Body::empty()).unwrap(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/html; charset=utf-8"
            );
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert_eq!(&body[..], include_bytes!("../public/404.html"));
        }

        for path in [
            "/",
            "/game?seed=123",
            "/online/game/01234567-89ab-4def-8123-456789abcdef",
            "/join/ABCDEF",
        ] {
            let response = render_page(
                State(state.clone()),
                Request::builder().uri(path).body(Body::empty()).unwrap(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert!(std::str::from_utf8(&body)
                .unwrap()
                .contains("SSR route reached"));
        }
    }
    async fn read_room(response: Response) -> tantrix_proto::RoomReference {
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn http_creation_retries_are_scoped_to_the_player_and_resolve_to_one_game() {
        let state = Arc::new(ServerState {
            rooms: room::RoomManager::new(store::RoomStore::open(":memory:").unwrap()),
            public_origin: Some("https://example.com".into()),
        });
        let request = CreateRoom {
            request_id: "01234567-89ab-4def-8123-456789abcdef".into(),
        };
        let token = SessionToken::new("player-one-123456");
        let (a, b) = tokio::join!(
            create_room(
                State(state.clone()),
                Extension(token.clone()),
                Json(request.clone())
            ),
            create_room(
                State(state.clone()),
                Extension(token),
                Json(request.clone())
            ),
        );
        let a = read_room(a).await;
        assert_eq!(a, read_room(b).await);
        let code = a.code.clone().unwrap();
        assert_eq!(
            read_room(resolve_room(State(state.clone()), Path(code)).await).await,
            a
        );
        let other = read_room(
            create_room(
                State(state.clone()),
                Extension(SessionToken::new("player-two-123456")),
                Json(request),
            )
            .await,
        )
        .await;
        assert_ne!(a.id, other.id);
        assert_ne!(a.code, other.code);
        let reference =
            read_room(room_reference(State(state.clone()), Path(a.id.clone())).await).await;
        assert_eq!(reference, a);
        let missing = room_reference(
            State(state),
            Path("00000000-0000-4000-8000-000000000000".into()),
        )
        .await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_creation_requests_cannot_allocate_rooms() {
        let state = Arc::new(ServerState {
            rooms: room::RoomManager::new(store::RoomStore::open(":memory:").unwrap()),
            public_origin: Some("https://example.com".into()),
        });
        let token = SessionToken::new("player-one-123456");
        assert_eq!(
            create_room(
                State(state.clone()),
                Extension(token),
                Json(CreateRoom {
                    request_id: "ABCDEF".into()
                })
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            resolve_room(State(state), Path("bad".into()))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
}
