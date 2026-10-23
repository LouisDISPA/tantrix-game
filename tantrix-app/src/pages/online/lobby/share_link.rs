//! Build a shareable URL of the shape `<origin>/online/game/ID`. Falls back to
//! a relative path for SSR. Desktop uses the room's captured server origin.

pub fn share_link(id: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(origin) = win.location().origin() {
                return format!("{origin}/online/game/{id}");
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
        format!("{}/online/game/{id}", server.0)
    }
    #[cfg(not(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    )))]
    format!("/online/game/{id}")
}
