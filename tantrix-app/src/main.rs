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
    #[cfg(feature = "desktop")]
    velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .run();
    tantrix_app::init_logging();
    #[cfg(feature = "desktop")]
    dioxus::LaunchBuilder::desktop()
        .with_cfg(tantrix_app::desktop::config())
        .launch(tantrix_app::app);
    #[cfg(all(feature = "mobile", not(feature = "desktop")))]
    dioxus::LaunchBuilder::mobile()
        .with_cfg(tantrix_app::mobile::config())
        .launch(tantrix_app::app);
    #[cfg(not(any(feature = "desktop", feature = "mobile")))]
    dioxus::launch(tantrix_app::app);
}
