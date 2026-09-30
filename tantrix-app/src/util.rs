mod logger;

// The server binary never runs the client, so it never calls this.
#[cfg_attr(feature = "server", allow(unused_imports))]
pub use logger::init_logging;

use std::time::Duration;

#[cfg(feature = "web")]
pub async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

#[cfg(not(feature = "web"))]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}
