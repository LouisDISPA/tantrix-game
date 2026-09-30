//! Server-side rendering of the Tantrix app on Cloudflare Workers.
//!
//! Every HTML request is rendered by Dioxus's own fullstack renderer
//! (`dioxus-server`, patched for wasm32), so the page carries the same
//! hydration markers and `initial_dioxus_hydration_data` payload as the native
//! fullstack server, and the web client (built with `fullstack`) hydrates it.
//!
//! This module is called by the JS entry point (`cloudflare/src/index.js`),
//! which keeps signaling and static assets; see `cloudflare/README.md`.

#![cfg(target_arch = "wasm32")]

use std::sync::LazyLock;

use axum::body::Body;
use axum::http::Response;
use axum::Router;
use dioxus_server::{DioxusRouterExt, IndexHtml, ServeConfig};
use tower_service::Service;
use worker::{event, Context, Env, HttpRequest};

const INDEX_HTML: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

/// Built once per isolate: parsing `index.html` and the renderer pool are
/// immutable across requests.
static ROUTER: LazyLock<Router> = LazyLock::new(|| {
    let index = IndexHtml::new(INDEX_HTML, "main").expect("client index.html is missing #main");
    // No static assets here: the Workers static assets binding serves them.
    Router::new().serve_api_application(ServeConfig::with_index_html(index), tantrix_app::app)
});

#[event(start)]
fn start() {
    console_error_panic_hook::set_once();
}

#[event(fetch)]
async fn fetch(request: HttpRequest, _env: Env, _ctx: Context) -> worker::Result<Response<Body>> {
    Ok(ROUTER.clone().call(request).await?)
}
