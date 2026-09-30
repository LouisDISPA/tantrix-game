//! Fullstack server entrypoint.
//!
//! Serves the Dioxus app and native online rooms, with **server-side rendering
//! and hydration** out of the box — `dioxus::server::router(app)` returns an
//! Axum router that renders each route on the server into HTML, ships a
//! hydration payload, and the wasm bundle picks up interactivity on the
//! client. See https://dioxuslabs.com/learn/0.7/essentials/fullstack/ssr.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ws::WebSocketUpgrade, Extension, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tantrix_room::session_cookie::{self, SessionToken};

use crate::app;

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
        let router = Router::new()
            .route("/room/{code}", get(room_socket))
            .route("/health", get(|| async { "OK" }))
            .with_state(state.clone())
            .merge(dioxus::server::router(app))
            .layer(from_fn_with_state(state, session_cookie_layer));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("failed to bind fullstack server");
        axum::serve(listener, router)
            .await
            .expect("fullstack server exited with an error");
    });
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
    let existing = session_cookie::read(cookie);
    let token = existing.clone().unwrap_or_else(SessionToken::generate);
    request.extensions_mut().insert(token.clone());
    let mut response = next.run(request).await;
    if existing.is_none() {
        let secure = state
            .public_origin
            .as_deref()
            .is_some_and(|origin| origin.starts_with("https://"));
        if let Ok(value) = HeaderValue::from_str(&session_cookie::set_cookie(&token, secure)) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

async fn room_socket(
    State(state): State<Arc<ServerState>>,
    Path(code): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    Extension(token): Extension<SessionToken>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !room::valid_code(&code) {
        return (StatusCode::BAD_REQUEST, "invalid room code").into_response();
    }
    let expected_origin = state.public_origin.clone().or_else(|| {
        headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .map(|host| format!("http://{host}"))
    });
    if headers.get(header::ORIGIN).is_some_and(|origin| {
        expected_origin
            .as_deref()
            .is_none_or(|expected| origin.as_bytes() != expected.as_bytes())
    }) {
        return (StatusCode::FORBIDDEN, "cross-origin room connection").into_response();
    }

    let create = query.get("create").is_some_and(|value| value == "1");
    match state.rooms.open(&code, create).await {
        Ok(room::OpenRoom::Ready(handle)) => {
            upgrade.on_upgrade(move |socket| room::socket_session(socket, handle, token))
        }
        Ok(room::OpenRoom::Missing) => upgrade.on_upgrade(room::missing_room),
        Ok(room::OpenRoom::Collision) => {
            (StatusCode::CONFLICT, "room code already exists").into_response()
        }
        Err(error) => {
            log::error!("failed to open room {code}: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "room storage is unavailable",
            )
                .into_response()
        }
    }
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
