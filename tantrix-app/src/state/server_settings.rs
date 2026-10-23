//! Shared online server address validation and defaults.

pub const DEFAULT_SERVER: &str = "https://tantrix.louis-dispa.fr";

/// Store an origin, so socket URLs and invitation links agree about the server.
pub fn normalize_server(address: &str) -> Result<String, String> {
    let address = address.trim();
    if address.is_empty() {
        return Err("Enter a server address.".into());
    }
    let address = if address.contains("://") {
        address.to_owned()
    } else {
        format!("https://{address}")
    };
    let mut url = url::Url::parse(&address)
        .map_err(|_| "Enter a valid hostname or HTTP(S) address.".to_string())?;
    let scheme = match url.scheme() {
        "https" | "wss" => "https",
        "http" | "ws" => "http",
        _ => return Err("Use an HTTP(S) or WebSocket server address.".into()),
    };
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("Use just the server address and optional port, without credentials, a path, or a query.".into());
    }
    url.set_scheme(scheme)
        .map_err(|_| "Invalid server scheme.".to_string())?;
    Ok(url.origin().ascii_serialization())
}

#[cfg(any(feature = "desktop", target_os = "android"))]
use super::native_settings::{load, save};

#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
pub fn server_address() -> String {
    load("server-address")
        .and_then(|address| normalize_server(&address).ok())
        .unwrap_or_else(|| DEFAULT_SERVER.into())
}

#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
pub fn save_server_address(address: &str) -> Result<String, String> {
    let address = normalize_server(address)?;
    save("server-address", &address)?;
    Ok(address)
}

/// Keep private reconnect identities separate for each server on native clients.
#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android")
))]
pub fn session_cookie(server: &str) -> Result<String, String> {
    let filename = session_key(server)?;
    let token = match load(&filename) {
        Some(token) if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) => {
            token
        }
        _ => {
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes)
                .map_err(|error| format!("Could not create player identity: {error}"))?;
            let token = bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            save(&filename, &token)?;
            token
        }
    };
    Ok(format!("tantrix_session={token}"))
}

/// HTTP preflight and WebSocket connections to one origin share an identity.
#[cfg(any(
    all(feature = "desktop", not(target_arch = "wasm32")),
    all(feature = "android", target_os = "android"),
    test
))]
fn session_key(server: &str) -> Result<String, String> {
    let url = url::Url::parse(server).map_err(|error| error.to_string())?;
    let address = normalize_server(&url.origin().ascii_serialization())?;
    let key = address
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("session-{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_identity_is_scoped_to_the_server_origin() {
        let https = session_key("https://example.com").unwrap();
        assert_eq!(
            https,
            session_key("wss://example.com/room/01234567-89ab-4def-8123-456789abcdef").unwrap()
        );
        assert_ne!(https, session_key("https://other.example.com").unwrap());
        assert_ne!(https, session_key("https://example.com:8443").unwrap());
        assert_ne!(https, session_key("http://example.com").unwrap());
        assert!(session_key("not a URL").is_err());
    }

    #[test]
    fn addresses_normalize_to_origins() {
        for (input, expected) in [
            (" tantrix.louis-dispa.fr ", DEFAULT_SERVER),
            ("wss://tantrix.louis-dispa.fr/", DEFAULT_SERVER),
            ("http://localhost:8080", "http://localhost:8080"),
            ("ws://127.0.0.1:8787/", "http://127.0.0.1:8787"),
            ("http://[::1]:8080", "http://[::1]:8080"),
        ] {
            assert_eq!(normalize_server(input).unwrap(), expected);
        }
        for invalid in [
            "",
            "ftp://example.com",
            "https://user:secret@example.com",
            "https://example.com/room",
            "https://example.com?create=1",
            "https://example.com#room",
            "not a host",
        ] {
            assert!(normalize_server(invalid).is_err(), "{invalid}");
        }
    }
}
