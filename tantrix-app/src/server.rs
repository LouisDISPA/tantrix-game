//! Fullstack server entrypoint.
//!
//! Serves the Dioxus app and an embedded matchbox signaling server on a
//! single port. Matchbox routes live under `/signaling/*`; everything else
//! falls through to the Dioxus handler, which does **server-side rendering
//! + hydration** out of the box — `dioxus::server::router(app)` returns an
//! Axum router that renders each route on the server into HTML, ships a
//! hydration payload, and the wasm bundle picks up interactivity on the
//! client. See https://dioxuslabs.com/learn/0.7/essentials/fullstack/ssr.

use std::net::SocketAddr;

use axum::Router;
use matchbox_signaling::SignalingServer;

use crate::app;

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
        let dioxus_router = dioxus::server::router(app);
        let server = build_server(addr, dioxus_router);
        server
            .serve()
            .await
            .expect("fullstack server exited with an error");
    });
}

fn setup_logging() {
    use tracing_subscriber::prelude::*;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "tantrix_app=info,matchbox_signaling=info,tower_http=info".into());
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().compact())
        .init();
}

/// Build the combined matchbox + Dioxus server. Matchbox is built via its
/// full-mesh builder (the same topology matchbox_server ships by default),
/// then composed with the Dioxus router at build time: `/signaling/*` is
/// served by matchbox, everything else by Dioxus.
fn build_server(addr: SocketAddr, dioxus_router: Router) -> SignalingServer {
    SignalingServer::full_mesh_builder(addr)
        .on_peer_connected(|peer_id| {
            println!("peer connected: {peer_id}");
        })
        .on_peer_disconnected(|peer_id| {
            println!("peer disconnected: {peer_id}");
        })
        .cors()
        .trace()
        .build_with(move |signaling_router| {
            Router::new()
                .nest("/signaling", signaling_router)
                .merge(dioxus_router)
        })
}
