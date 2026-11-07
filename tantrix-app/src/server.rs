//! Fullstack server entrypoint.
//!
//! Serves the Dioxus app and native online rooms, with **server-side rendering
//! and hydration** through Dioxus's Axum handlers. Known application routes
//! render on the server; unknown paths return the shared static 404 page.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::{
    ws::WebSocketUpgrade, ConnectInfo, Extension, FromRequestParts, Path, RawQuery, State,
};
use axum::http::{header, request::Parts, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use dioxus::server::{DioxusRouterExt, FullstackState, ServeConfig};
use tantrix_proto::{is_game_id, CreateRoom, RejectCode, RoomApiError};
use tantrix_room::handshake::{self, Refusal, SiteOrigin, Upgrade};
use tantrix_room::lifecycle;
use tantrix_room::session_cookie::{self, SessionToken};
#[cfg(not(debug_assertions))]
use tower_http::compression::CompressionLayer;
use tracing::Instrument;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::app;
use crate::route::Route;

mod limits;
mod room;
mod store;

struct ServerState {
    rooms: room::RoomManager,
    /// Set behind a reverse proxy, which is then trusted for client addresses.
    public_origin: Option<String>,
    limits: limits::RoomLimits,
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
    tracing::info!(%addr, "Tantrix fullstack server listening");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    runtime.block_on(async move {
        let db_path =
            std::env::var("TANTRIX_ROOM_DB").unwrap_or_else(|_| "tantrix-rooms.sqlite3".into());
        let store = store::RoomStore::open(&db_path).expect("failed to open room database");
        store
            .mark_vacant(room::now_ms())
            .await
            .expect("failed to mark rooms vacant");
        let state = Arc::new(ServerState {
            rooms: room::RoomManager::new(store),
            public_origin: std::env::var("TANTRIX_PUBLIC_ORIGIN")
                .ok()
                .map(|origin| origin.trim_end_matches('/').to_owned()),
            limits: limits::RoomLimits::new(),
        });
        let cleanup_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Err(error) = cleanup_state.rooms.cleanup(room::now_ms()).await {
                    tracing::error!(%error, "room cleanup failed");
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
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
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

/// The address a room request is counted under: see [`limits::client_ip`].
struct Client(IpAddr);

impl FromRequestParts<Arc<ServerState>> for Client {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<ServerState>,
    ) -> Result<Self, Self::Rejection> {
        // The server always has connect info; without it everyone shares one count.
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |info| info.0.ip());
        let trust_forwarded = state.public_origin.is_some();
        Ok(Self(limits::client_ip(
            peer,
            &parts.headers,
            trust_forwarded,
        )))
    }
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
    Client(client): Client,
    Extension(token): Extension<SessionToken>,
    Json(request): Json<CreateRoom>,
) -> Response {
    if !state.limits.allow_creation(client) {
        tracing::warn!(%client, limit = "room creation", "request rate limited");
        return api_error(RoomApiError::RateLimited);
    }
    if !is_game_id(&request.request_id) {
        return api_error(RoomApiError::InvalidRequest);
    }
    let id = lifecycle::game_id(&token, &request.request_id);
    api_result(state.rooms.create(&id, room::now_ms()).await)
}

async fn resolve_room(
    State(state): State<Arc<ServerState>>,
    Client(client): Client,
    Path(code): Path<String>,
) -> Response {
    if !state.limits.allow_lookup(client) {
        tracing::warn!(%client, limit = "code lookup", "request rate limited");
        return api_error(RoomApiError::RateLimited);
    }
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
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let text = |name| headers.get(name).map(|value| value.to_str().unwrap_or(""));
    // Without a configured public origin (local development), the page must
    // come from the host it asked.
    let site = match &state.public_origin {
        Some(origin) => SiteOrigin::Exact(origin),
        None => SiteOrigin::Host(text(header::HOST).unwrap_or("")),
    };
    let checked = handshake::check(&Upgrade {
        id: &code,
        origin: text(header::ORIGIN),
        site,
        query: query.as_deref(),
    });
    if let Err(refusal) = checked {
        // The ID may be invalid, so it stays out of the logs.
        tracing::info!(reason = refusal.message(), "socket refused");
        if refusal.closes_socket() {
            return upgrade.on_upgrade(move |socket| room::refused(socket, refusal.message()));
        }
        return refuse(refusal);
    }
    // Larger frames never reach the room (axum allows 64 MiB by default).
    let upgrade = upgrade
        .max_message_size(tantrix_room::engine::MAX_FRAME_BYTES)
        .max_frame_size(tantrix_room::engine::MAX_FRAME_BYTES);

    let now = room::now_ms();
    let span = room::room_span(&code);
    let opened = state.rooms.open(&code, now).instrument(span.clone()).await;
    let rejected = match opened {
        Ok(room::OpenRoom::Ready(handle)) => match handle.inspect(now, true).await {
            Ok(_) => {
                return upgrade
                    .on_upgrade(move |socket| room::socket_session(socket, handle, token))
            }
            Err(RoomApiError::Expired) => RejectCode::RoomExpired,
            Err(RoomApiError::NotFound) => RejectCode::RoomNotFound,
            Err(error) => {
                span.in_scope(|| tracing::warn!(?error, "socket refused"));
                return api_error(error);
            }
        },
        Ok(room::OpenRoom::Missing) => RejectCode::RoomNotFound,
        Ok(room::OpenRoom::Expired) => RejectCode::RoomExpired,
        Err(error) => {
            span.in_scope(|| tracing::error!(%error, "room storage error"));
            return refuse(Refusal::StorageUnavailable);
        }
    };
    span.in_scope(|| tracing::info!(?rejected, "socket rejected"));
    upgrade.on_upgrade(move |socket| room::rejected_room(socket, rejected))
}

fn refuse(refusal: Refusal) -> Response {
    let status =
        StatusCode::from_u16(refusal.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, refusal.message()).into_response()
}

/// How the server writes logs to stdout; `RUST_LOG` filters either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogFormat {
    /// One line of text per event, then its fields and spans' fields.
    Compact,
    /// One JSON object per line: `TANTRIX_LOG_FORMAT=json`.
    Json,
}

impl LogFormat {
    /// The format `TANTRIX_LOG_FORMAT` names, or `None` for an unknown one.
    fn parse(value: Option<&str>) -> Option<Self> {
        match value.map(str::trim) {
            None | Some("" | "compact") => Some(Self::Compact),
            Some("json") => Some(Self::Json),
            Some(_) => None,
        }
    }
}

/// Events and their spans' fields, written as `format` lines to `writer`.
fn log_layer<S, W>(format: LogFormat, writer: W) -> Box<dyn Layer<S> + Send + Sync>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let layer = tracing_subscriber::fmt::layer().with_writer(writer);
    match format {
        LogFormat::Compact => layer.compact().boxed(),
        // Event fields sit beside `message`; `span` is the innermost span.
        LogFormat::Json => layer.json().flatten_event(true).boxed(),
    }
}

fn setup_logging() {
    use tracing_subscriber::prelude::*;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "tantrix_app=info,tower_http=info".into());
    let requested = std::env::var("TANTRIX_LOG_FORMAT").ok();
    let format = LogFormat::parse(requested.as_deref());
    tracing_subscriber::registry()
        .with(filter)
        .with(log_layer(
            format.unwrap_or(LogFormat::Compact),
            std::io::stdout,
        ))
        .init();
    if format.is_none() {
        tracing::warn!(
            value = requested.as_deref(),
            "unknown TANTRIX_LOG_FORMAT; using compact"
        );
    }
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

    const CLIENT: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));

    fn test_state() -> Arc<ServerState> {
        Arc::new(ServerState {
            rooms: room::RoomManager::new(store::RoomStore::open(":memory:").unwrap()),
            public_origin: Some("https://example.com".into()),
            limits: limits::RoomLimits::new(),
        })
    }

    async fn read_room(response: Response) -> tantrix_proto::RoomReference {
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn http_creation_retries_are_scoped_to_the_player_and_resolve_to_one_game() {
        let state = test_state();
        let request = CreateRoom {
            request_id: "01234567-89ab-4def-8123-456789abcdef".into(),
        };
        let token = SessionToken::new("player-one-123456");
        let (a, b) = tokio::join!(
            create_room(
                State(state.clone()),
                Client(CLIENT),
                Extension(token.clone()),
                Json(request.clone())
            ),
            create_room(
                State(state.clone()),
                Client(CLIENT),
                Extension(token),
                Json(request.clone())
            ),
        );
        let a = read_room(a).await;
        assert_eq!(a, read_room(b).await);
        let code = a.code.clone().unwrap();
        assert_eq!(
            read_room(resolve_room(State(state.clone()), Client(CLIENT), Path(code)).await).await,
            a
        );
        let other = read_room(
            create_room(
                State(state.clone()),
                Client(CLIENT),
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

    const MISSING_GAME: &str = "00000000-0000-4000-8000-000000000000";

    /// A server with only the room socket route, on a free local port.
    async fn serve_sockets(public_origin: Option<&str>) -> SocketAddr {
        serve_state(Arc::new(ServerState {
            rooms: room::RoomManager::new(store::RoomStore::open(":memory:").unwrap()),
            public_origin: public_origin.map(str::to_owned),
            limits: limits::RoomLimits::new(),
        }))
        .await
    }

    /// `state`'s room socket route, on a free local port.
    async fn serve_state(state: Arc<ServerState>) -> SocketAddr {
        let router = Router::new()
            .route("/room/{id}", get(room_socket))
            .with_state(state.clone())
            .layer(from_fn_with_state(state, session_cookie_layer));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        address
    }

    /// The first frame of an accepted socket, or the status refusing it.
    async fn first_frame(
        url: &str,
        origin: Option<&str>,
    ) -> Result<tokio_tungstenite::tungstenite::Message, u16> {
        use futures::StreamExt;
        use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Error};
        let mut request = url.into_client_request().unwrap();
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert(header::ORIGIN, origin.parse().unwrap());
        }
        match tokio_tungstenite::connect_async(request).await {
            Ok((mut socket, _)) => Ok(socket.next().await.unwrap().unwrap()),
            Err(Error::Http(response)) => Err(response.status().as_u16()),
            Err(error) => panic!("{error}"),
        }
    }

    #[tokio::test]
    async fn room_sockets_accept_their_own_site_and_native_apps_only() {
        use crate::state::server_endpoints::room_url;
        // Accepted sockets hear that this game is missing, in a text frame.
        let accepted =
            |frame| matches!(frame, Ok(tokio_tungstenite::tungstenite::Message::Text(_)));

        let local = serve_sockets(None).await;
        let own_site = format!("http://{local}");
        let url = room_url(&own_site, MISSING_GAME);
        assert!(accepted(first_frame(&url, None).await));
        assert!(accepted(first_frame(&url, Some(&own_site)).await));
        assert_eq!(
            first_frame(&url, Some("https://evil.example")).await,
            Err(403)
        );

        let public = serve_sockets(Some("https://tantrix.example.com")).await;
        let url = room_url(&format!("http://{public}"), MISSING_GAME);
        assert!(accepted(first_frame(&url, None).await));
        assert!(accepted(
            first_frame(&url, Some("https://tantrix.example.com")).await
        ));
        // Once the public origin is known, the server's own address is not it.
        assert_eq!(
            first_frame(&url, Some(&format!("http://{public}"))).await,
            Err(403)
        );
    }

    #[tokio::test]
    async fn outdated_clients_read_why_their_socket_closed() {
        use tantrix_proto::{ONLINE_VERSION, ONLINE_VERSION_PARAM};
        use tokio_tungstenite::tungstenite::Message;
        let address = serve_sockets(None).await;
        for (version, refusal) in [
            (ONLINE_VERSION - 1, Refusal::AppOutdated),
            (ONLINE_VERSION + 1, Refusal::ServerOutdated),
        ] {
            let url =
                format!("ws://{address}/room/{MISSING_GAME}?{ONLINE_VERSION_PARAM}={version}");
            let Ok(Message::Close(Some(frame))) = first_frame(&url, None).await else {
                panic!("version {version} was not refused");
            };
            assert_eq!(frame.reason, refusal.message());
        }
    }

    #[tokio::test]
    async fn invalid_creation_requests_cannot_allocate_rooms() {
        let state = test_state();
        let token = SessionToken::new("player-one-123456");
        assert_eq!(
            create_room(
                State(state.clone()),
                Client(CLIENT),
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
            resolve_room(State(state), Client(CLIENT), Path("bad".into()))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn a_client_over_its_limits_gets_a_retryable_429() {
        let state = test_state();
        let token = SessionToken::new("player-one-123456");
        let create = |client: IpAddr, n: usize| {
            create_room(
                State(state.clone()),
                Client(client),
                Extension(token.clone()),
                Json(CreateRoom {
                    request_id: format!("01234567-89ab-4def-8123-{n:012x}"),
                }),
            )
        };
        for n in 0..10 {
            assert_eq!(create(CLIENT, n).await.status(), StatusCode::OK);
        }
        let refused = create(CLIENT, 10).await;
        assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(refused.headers()["cache-control"], "no-store");
        let body = to_bytes(refused.into_body(), 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<RoomApiError>(&body).unwrap(),
            RoomApiError::RateLimited
        );
        // Other clients, and invitation lookups, keep their own counts.
        let other = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));
        assert_eq!(create(other, 11).await.status(), StatusCode::OK);
        let lookup = || resolve_room(State(state.clone()), Client(CLIENT), Path("ABCDEF".into()));
        for _ in 0..30 {
            assert_eq!(lookup().await.status(), StatusCode::NOT_FOUND);
        }
        assert_eq!(lookup().await.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    thread_local! {
        /// The format and buffer of the log capture on this test's thread.
        static CAPTURE: std::cell::RefCell<Option<(LogFormat, Buffer)>> =
            const { std::cell::RefCell::new(None) };
    }

    /// Bytes a capture's log layer wrote.
    #[derive(Clone, Default)]
    struct Buffer(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Writes each event to the capture on the thread that logged it.
    struct ThreadCapture;

    impl MakeWriter<'_> for ThreadCapture {
        type Writer = tracing_subscriber::fmt::writer::OptionalWriter<Buffer>;

        fn make_writer(&self) -> Self::Writer {
            CAPTURE
                .with_borrow(|capture| capture.as_ref().map(|(_, buffer)| buffer.clone()))
                .into()
        }
    }

    /// A `format` layer that sees the server's own events on capturing threads.
    fn capture_layer<S>(format: LogFormat) -> impl Layer<S>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span>,
    {
        // Dynamic, so `tracing` asks on every event instead of caching.
        let filter = tracing_subscriber::filter::dynamic_filter_fn(move |metadata, _| {
            metadata.target().starts_with("tantrix_app")
                && CAPTURE.with_borrow(|capture| matches!(capture, Some((f, _)) if *f == format))
        });
        log_layer(format, ThreadCapture).with_filter(filter)
    }

    /// The server's own events, every level, that this thread logs while the
    /// capture lives.
    ///
    /// Captures share one global subscriber rather than each setting a
    /// thread's default: `tracing` caches per call site whether any subscriber
    /// wants it, and while a test's scoped subscriber is the only one, the
    /// cache follows whichever thread reaches the call site first. A parallel
    /// test without a subscriber then turns that event off for good.
    struct Logs(Buffer);

    impl Logs {
        fn capture(format: LogFormat) -> Self {
            static SUBSCRIBER: std::sync::Once = std::sync::Once::new();
            SUBSCRIBER.call_once(|| {
                use tracing_subscriber::prelude::*;
                let subscriber = tracing_subscriber::registry()
                    .with(capture_layer(LogFormat::Compact))
                    .with(capture_layer(LogFormat::Json));
                tracing::subscriber::set_global_default(subscriber)
                    .expect("no other global subscriber in tests");
            });
            let buffer = Buffer::default();
            CAPTURE.set(Some((format, buffer.clone())));
            Self(buffer)
        }

        fn text(&self) -> String {
            String::from_utf8(self.0 .0.lock().unwrap().clone()).unwrap()
        }

        /// The first JSON event with `message`, once a server task logs it.
        async fn event(&self, message: &str) -> serde_json::Value {
            for _ in 0..1000 {
                let text = self.text();
                let mut events = text.lines().map(|line| {
                    serde_json::from_str::<serde_json::Value>(line)
                        .expect("one JSON object per line")
                });
                if let Some(event) = events.find(|event| event["message"] == message) {
                    return event;
                }
                tokio::task::yield_now().await;
            }
            panic!("no {message:?} event in:\n{}", self.text());
        }
    }

    impl Drop for Logs {
        fn drop(&mut self) {
            CAPTURE.set(None);
        }
    }

    #[tokio::test]
    async fn socket_logs_name_the_room_connection_and_reason_but_not_the_token() {
        use futures::{SinkExt, StreamExt};
        use tantrix_proto::{HostMessage, PeerMessage};
        use tantrix_room::engine::{MESSAGE_BURST, TOO_MANY_MESSAGES};
        use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

        const GAME: &str = "01234567-89ab-4def-8123-456789abcdef";
        const TOKEN: &str = "private-session-token-1234";
        // The test's runtime runs every server task on this thread.
        let logs = Logs::capture(LogFormat::Json);
        let state = test_state();
        state.rooms.create(GAME, room::now_ms()).await.unwrap();
        let address = serve_state(state).await;

        let url = crate::state::server_endpoints::room_url(&format!("http://{address}"), GAME);
        let mut request = url.into_client_request().unwrap();
        let cookie = format!("{}={TOKEN}", session_cookie::COOKIE_NAME);
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        let hello = PeerMessage::Hello {
            nickname: "Player".into(),
        };
        let hello = serde_json::to_string(&hello).unwrap();
        socket.send(Message::text(hello)).await.unwrap();
        let peer = loop {
            let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
                continue;
            };
            if let HostMessage::Welcome { your_peer_id, .. } = serde_json::from_str(&text).unwrap()
            {
                break your_peer_id;
            }
        };
        // Invalid frames, each answered before the next, until the room
        // closes the socket: nothing is left unread when it does.
        let mut reason = None;
        for _ in 0..4 * MESSAGE_BURST {
            socket.send(Message::text("{}")).await.unwrap();
            match socket.next().await {
                Some(Ok(Message::Text(_))) => {}
                Some(Ok(Message::Close(Some(frame)))) => {
                    reason = Some(frame.reason.to_string());
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(reason.as_deref(), Some(TOO_MANY_MESSAGES));

        let created = logs.event("room created").await;
        assert_eq!(created["span"]["room"], GAME);
        assert_eq!(logs.event("room loaded").await["span"]["room"], GAME);
        let opened = logs.event("socket opened").await;
        assert_eq!(opened["span"]["room"], GAME);
        assert!(opened["span"]["peer"].is_null(), "not joined yet");
        let conn = opened["span"]["conn"].as_u64().unwrap();
        let joined = logs.event("socket joined").await;
        let closed = logs.event("socket closed").await;
        for event in [&joined, &closed] {
            assert_eq!(event["span"]["name"], "socket");
            assert_eq!(event["span"]["room"], GAME);
            assert_eq!(event["span"]["conn"], conn);
            assert_eq!(event["span"]["peer"], peer.as_str());
        }
        assert_eq!(closed["level"], "WARN");
        assert_eq!(closed["reason"], TOO_MANY_MESSAGES);
        assert!(!logs.text().contains(TOKEN), "{}", logs.text());
    }

    #[test]
    fn compact_logs_carry_the_socket_fields() {
        use tantrix_room::engine::{ConnId, TOO_MANY_CONNECTIONS};

        let logs = Logs::capture(LogFormat::Compact);
        let span = room::socket_span(MISSING_GAME, ConnId(7));
        span.record("peer", tracing::field::display("peer-7"));
        span.in_scope(|| room::log_closed(TOO_MANY_CONNECTIONS));
        // Strip colours: `ESC [ … m`.
        let mut line = String::new();
        let mut chars = logs.text().chars().collect::<Vec<_>>().into_iter();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                chars.find(|&c| c == 'm');
            } else {
                line.push(c);
            }
        }
        for part in [
            "WARN",
            "socket closed",
            &format!("reason={TOO_MANY_CONNECTIONS:?}"),
            &format!("room={MISSING_GAME}"),
            "conn=7",
            "peer=peer-7",
        ] {
            assert!(line.contains(part), "{part} missing from {line}");
        }
    }

    #[test]
    fn logs_are_compact_unless_json_is_asked_for() {
        assert_eq!(LogFormat::parse(None), Some(LogFormat::Compact));
        assert_eq!(LogFormat::parse(Some("compact")), Some(LogFormat::Compact));
        assert_eq!(LogFormat::parse(Some(" json")), Some(LogFormat::Json));
        assert_eq!(LogFormat::parse(Some("yaml")), None);
    }
}
