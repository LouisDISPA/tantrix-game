mod logger;
#[cfg(test)]
pub(crate) mod render_count;
#[cfg(test)]
pub(crate) mod test_page;

// The server binary never runs the client, so it never calls this.
#[cfg_attr(feature = "server", allow(unused_imports))]
pub use logger::init_logging;

use std::time::Duration;

/// Browser builds (the web client, the Worker's server-side rendering) wait
/// on JS timers.
#[cfg(any(feature = "web", feature = "worker"))]
pub(crate) async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

/// The native server and renderers run on tokio, which they enable.
#[cfg(all(
    not(any(feature = "web", feature = "worker")),
    any(feature = "server", feature = "desktop", feature = "mobile")
))]
pub(crate) async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// A build without a platform feature (`cargo test -p tantrix-app`) has no
/// runtime of its own: a thread waits, then wakes the task. Native only in
/// practice: wasm32 builds always have `web` or `worker`.
#[cfg(not(any(
    feature = "web",
    feature = "worker",
    feature = "server",
    feature = "desktop",
    feature = "mobile"
)))]
pub(crate) async fn sleep(duration: Duration) {
    let (done, wait) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        // The task may be gone: then nobody waits.
        let _ = done.send(());
    });
    let _ = wait.await;
}
