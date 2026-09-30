//! Routes the `log` crate to the browser console.
//!
//! Dioxus only installs a `tracing` subscriber, so every `log::…!` call in
//! `matchbox_socket` and `tantrix-net` was silently discarded. That is where
//! the interesting online-play events live: signaling ids, SDP offer/answer,
//! ICE gathering and connection state, data-channel open/close, and the join
//! timeouts. With this logger they show up in DevTools (and in the e2e test
//! timelines) at the level chosen in [`max_level_for`].

/// Install the console logger. Safe to call more than once.
#[cfg(target_arch = "wasm32")]
pub fn init_logging() {
    static LOGGER: ConsoleLogger = ConsoleLogger;
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

/// No-op off the browser: the server build has its own `tracing` setup.
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(feature = "server", allow(dead_code))]
pub fn init_logging() {}

/// The most verbose level shown for a log target.
///
/// The networking crates get `Debug` in every build: a handshake produces a
/// few dozen lines in total, and they are what is needed to diagnose a player
/// who cannot connect. Everything else stays at `Info`.
#[cfg(target_arch = "wasm32")]
fn max_level_for(target: &str) -> log::Level {
    const NETWORKING: [&str; 2] = ["matchbox_socket", "tantrix_net"];
    if NETWORKING.iter().any(|prefix| target.starts_with(prefix)) {
        log::Level::Debug
    } else {
        log::Level::Info
    }
}

#[cfg(target_arch = "wasm32")]
struct ConsoleLogger;

#[cfg(target_arch = "wasm32")]
impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= max_level_for(metadata.target())
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = console_line(record);
        match record.level() {
            log::Level::Error => web_sys::console::error_1(&line),
            log::Level::Warn => web_sys::console::warn_1(&line),
            log::Level::Info => web_sys::console::info_1(&line),
            log::Level::Debug | log::Level::Trace => web_sys::console::debug_1(&line),
        }
    }

    fn flush(&self) {}
}

#[cfg(target_arch = "wasm32")]
fn console_line(record: &log::Record) -> web_sys::wasm_bindgen::JsValue {
    web_sys::wasm_bindgen::JsValue::from_str(&format!("[{}] {}", record.target(), record.args()))
}
