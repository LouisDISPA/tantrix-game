//! Server-side rendering of the Tantrix app on Cloudflare Workers.
//!
//! Every HTML request is rendered by Dioxus's own fullstack renderer
//! (`dioxus-server`, patched for wasm32), so the page carries the same
//! hydration markers and `initial_dioxus_hydration_data` payload as the native
//! fullstack server, and the web client (built with `fullstack`) hydrates it.
//!
//! This module is called by the JS entry point (`cloudflare/src/index.js`),
//! which routes room WebSockets to the [`room::Room`] Durable Object and
//! static files to the assets binding; see `cloudflare/README.md`.

#![cfg(target_arch = "wasm32")]

use std::sync::LazyLock;

use axum::body::Body;
use axum::http::{header, HeaderValue, Response};
use axum::Router;
use dioxus_server::{DioxusRouterExt, IndexHtml, ServeConfig};
use tower_service::Service;
use worker::{event, Context, Env, HttpRequest};

mod room;
const INDEX_HTML: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

/// Built once per isolate: parsing `index.html` and the renderer pool are
/// immutable across requests.
static ROUTER: LazyLock<Router> = LazyLock::new(|| {
    let index = IndexHtml::new(INDEX_HTML, "main").expect("client index.html is missing #main");
    // No static assets here: the Workers static assets binding serves them.
    Router::new()
        .route(
            tantrix_proto::ONLINE_VERSION_PATH,
            axum::routing::get(|| async {
                (
                    [
                        ("cache-control", "no-store"),
                        ("content-type", "application/json"),
                    ],
                    tantrix_proto::encode(&tantrix_proto::OnlineVersion::CURRENT),
                )
            }),
        )
        .serve_api_application(ServeConfig::with_index_html(index), tantrix_app::app)
});

#[event(start)]
fn start() {
    console_error_panic_hook::set_once();
}

#[event(fetch)]
async fn fetch(request: HttpRequest, _env: Env, _ctx: Context) -> worker::Result<Response<Body>> {
    // Every visit starts on a server-rendered page, so this is where a new
    // browser gets its identity cookie, before it opens any room socket.
    let cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok());
    let secure = request.uri().scheme_str() == Some("https");
    let (_, set_cookie) = tantrix_room::session_cookie::identify(cookie, secure);
    let mut response = ROUTER.clone().call(request).await?;
    if let Some(value) = set_cookie.and_then(|value| HeaderValue::from_str(&value).ok()) {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    Ok(response)
}
