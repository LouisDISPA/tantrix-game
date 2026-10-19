//! Build a shareable URL of the shape `<origin>/join/CODE`. Falls back to
//! a relative path for SSR. Desktop uses the room's captured server origin.

pub fn share_link(code: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(origin) = win.location().origin() {
                return format!("{origin}/join/{code}");
            }
        }
    }
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    {
        let server =
            dioxus::prelude::consume_context::<crate::state::room_connection::RoomServer>();
        format!("{}/join/{code}", server.0)
    }
    #[cfg(not(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    )))]
    format!("/join/{code}")
}
