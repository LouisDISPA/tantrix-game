#![cfg_attr(
    all(feature = "bundle", target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(feature = "server")]
fn main() {
    tantrix_app::server::run();
}

#[cfg(not(feature = "server"))]
fn main() {
    tantrix_app::init_logging();
    #[cfg(feature = "desktop")]
    dioxus::LaunchBuilder::desktop()
        .with_cfg(tantrix_app::desktop::config())
        .launch(tantrix_app::app);
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(tantrix_app::app);
}
