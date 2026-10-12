//! Desktop preferences and private reconnect identities, stored per server.

use std::{fs, path::PathBuf};

pub const DEFAULT_SERVER: &str = "https://tantrix.louis-dispa.fr";

pub fn settings_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|path| path.join("tantrix"))
        .ok_or_else(|| "Could not locate the application settings directory.".into())
}

pub fn load(name: &str) -> Option<String> {
    fs::read_to_string(settings_dir().ok()?.join(name)).ok()
}

pub fn save(name: &str, value: &str) -> Result<(), String> {
    let directory = settings_dir()?;
    fs::create_dir_all(&directory).map_err(|error| format!("Could not save settings: {error}"))?;
    let path = directory.join(name);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options
        .open(path)
        .and_then(|mut file| file.write_all(value.as_bytes()))
        .map_err(|error| format!("Could not save settings: {error}"))
}

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

pub fn server_address() -> String {
    load("server-address")
        .and_then(|address| normalize_server(&address).ok())
        .unwrap_or_else(|| DEFAULT_SERVER.into())
}

pub fn session_cookie(server: &str) -> Result<String, String> {
    let url = url::Url::parse(server).map_err(|error| error.to_string())?;
    let address = normalize_server(&url.origin().ascii_serialization())?;
    // Encode the origin as a filename: distinct servers never share identity.
    let key = address
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let filename = format!("session-{key}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_addresses_normalize_to_origins() {
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
