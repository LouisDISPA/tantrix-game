mod logger;
#[cfg(test)]
pub mod render_count;

// The server binary never runs the client, so it never calls this.
#[cfg_attr(feature = "server", allow(unused_imports))]
pub use logger::init_logging;

use std::time::Duration;

/// Browser builds (the web client, the Worker's server-side rendering) wait
/// on JS timers.
#[cfg(any(feature = "web", feature = "worker"))]
pub async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

/// The native server and renderers run on tokio, which they enable.
#[cfg(all(
    not(any(feature = "web", feature = "worker")),
    any(
        feature = "server",
        feature = "desktop",
        feature = "native",
        feature = "mobile"
    )
))]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// A build without a platform feature (`cargo test -p tantrix-app`) has no
/// runtime of its own: wait on a runtime-agnostic timer thread.
#[cfg(not(any(
    feature = "web",
    feature = "worker",
    feature = "server",
    feature = "desktop",
    feature = "native",
    feature = "mobile"
)))]
pub async fn sleep(duration: Duration) {
    futures_timer::Delay::new(duration).await;
}
