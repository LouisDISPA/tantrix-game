//! Resolve the HTTP origin once, then derive the room's WebSocket endpoint.

/// Native clients use their saved setting. Browsers use the current origin,
/// unless a build-time TANTRIX_ROOM_URL points to another server's /room path.
pub fn server_origin() -> String {
    #[cfg(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    ))]
    return super::server_settings::server_address();

    #[cfg(not(any(
        all(feature = "desktop", not(target_arch = "wasm32")),
        all(feature = "android", target_os = "android")
    )))]
    {
        if let Some(address) = option_env!("TANTRIX_ROOM_URL") {
            let mut url = url::Url::parse(address).expect("TANTRIX_ROOM_URL must be a URL");
            let scheme = match url.scheme() {
                "wss" | "https" => "https",
                "ws" | "http" => "http",
                _ => panic!("TANTRIX_ROOM_URL must use HTTP(S) or WebSocket"),
            };
            url.set_scheme(scheme).expect("valid HTTP scheme");
            return url.origin().ascii_serialization();
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            if let Ok(origin) = window.location().origin() {
                return origin;
            }
        }
        "http://localhost:8787".into()
    }
}

/// `server` is the HTTP(S) origin used by the compatibility check and invites.
pub fn room_url(server: &str, room_code: &str, create: bool) -> String {
    let server = server.trim_end_matches('/');
    let socket_origin = server.replacen("http", "ws", 1);
    let query = if create { "?create=1" } else { "" };
    format!("{socket_origin}/room/{room_code}{query}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_endpoints_keep_the_origin_and_creation_intent() {
        for (origin, socket_origin) in [
            ("http://localhost:8080", "ws://localhost:8080"),
            ("https://example.com/", "wss://example.com"),
            ("https://example.com:8443", "wss://example.com:8443"),
            ("http://[::1]:8787", "ws://[::1]:8787"),
        ] {
            assert_eq!(
                room_url(origin, "BCDEFG", false),
                format!("{socket_origin}/room/BCDEFG")
            );
            assert_eq!(
                room_url(origin, "BCDEFG", true),
                format!("{socket_origin}/room/BCDEFG?create=1")
            );
        }
    }
}
