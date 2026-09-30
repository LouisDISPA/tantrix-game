#[cfg(feature = "server")]
fn main() {
    tantrix_app::server::run();
}

#[cfg(not(feature = "server"))]
fn main() {
    tantrix_app::init_logging();
    dioxus::launch(tantrix_app::app);
}
