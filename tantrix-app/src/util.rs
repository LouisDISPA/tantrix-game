use std::time::Duration;

#[cfg(feature = "web")]
pub async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

#[cfg(not(feature = "web"))]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}
