//! Build a shareable URL of the shape `<origin>/join/CODE`. Falls back to
//! a relative path if `window` isn't available (e.g. SSR or native).

pub fn share_link(code: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(origin) = win.location().origin() {
                return format!("{origin}/join/{code}");
            }
        }
    }
    format!("/join/{code}")
}
