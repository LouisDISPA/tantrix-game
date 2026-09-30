//! Single-threaded stand-in for `tokio_util::task::LocalPoolHandle` on wasm32.
//!
//! The upstream pool spawns one OS thread per core so each `!Send` `VirtualDom`
//! gets pinned to a thread. A Cloudflare Worker has exactly one thread (and
//! `std::thread::spawn` fails there), so "pinned" tasks simply run on the JS
//! event loop via `wasm_bindgen_futures::spawn_local`. The API mirrors the
//! subset of `LocalPoolHandle` / `tokio::task::JoinHandle` that the SSR and
//! server-function handlers use.

use futures_channel::oneshot;
use futures_util::future::{AbortHandle, Abortable};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Clone)]
pub struct LocalPoolHandle;

impl LocalPoolHandle {
    /// The pool size is ignored: there is only one thread.
    pub fn new(_pool_size: usize) -> Self {
        Self
    }

    pub fn spawn_pinned<F, Fut>(&self, create_task: F) -> JoinHandle<Fut::Output>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let (abort, registration) = AbortHandle::new_pair();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(output) = Abortable::new(create_task(), registration).await {
                let _ = tx.send(output);
            }
        });
        JoinHandle { rx, abort }
    }
}

/// Resolves to the task's output, or to `Canceled` if it was aborted.
pub struct JoinHandle<T> {
    rx: oneshot::Receiver<T>,
    abort: AbortHandle,
}

impl<T> JoinHandle<T> {
    pub fn abort(&self) {
        self.abort.abort();
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, oneshot::Canceled>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.rx).poll(cx)
    }
}
