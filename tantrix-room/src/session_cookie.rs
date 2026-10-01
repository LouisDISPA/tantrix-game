//! Browser identity for online rooms.
//!
//! Every browser carries one random token in an `HttpOnly` cookie. Pages set
//! it on first visit; the room reads it from the WebSocket handshake. Script
//! can't read it, and it never appears in a wire message, so other players
//! can't learn it and a reload or reconnect rebinds the same seat.

use serde::{Deserialize, Serialize};

pub const COOKIE_NAME: &str = "tantrix_session";

/// One year: long enough to come back to a game after closing the browser.
const MAX_AGE_SECS: u32 = 365 * 24 * 60 * 60;

/// Private per-browser identity. Only the server ever sees it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionToken(pub String);

impl SessionToken {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A fresh unguessable token (UUID v4 from the platform CSPRNG).
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// The session token in a `Cookie` request header, if it holds a
/// well-formed one. Anything else is treated as no cookie at all.
pub fn read(cookie_header: Option<&str>) -> Option<SessionToken> {
    cookie_header?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE_NAME)
        .map(|(_, value)| value.trim())
        .filter(|value| {
            (16..=64).contains(&value.len())
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        .map(SessionToken::new)
}

/// Who a request comes from: the token in its `Cookie` header, or a fresh
/// one with the `Set-Cookie` value that hands it to the browser. Pages call
/// this on every visit, and room upgrades as a fallback.
pub fn identify(cookie_header: Option<&str>, secure: bool) -> (SessionToken, Option<String>) {
    match read(cookie_header) {
        Some(token) => (token, None),
        None => {
            let token = SessionToken::generate();
            let cookie = set_cookie(&token, secure);
            (token, Some(cookie))
        }
    }
}

/// `Set-Cookie` value for `token`. `secure` must be true on HTTPS; plain
/// HTTP (`wrangler dev`) would drop a `Secure` cookie.
pub fn set_cookie(token: &SessionToken, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{COOKIE_NAME}={}; Path=/; Max-Age={MAX_AGE_SECS}; HttpOnly; SameSite=Lax{secure}",
        token.as_str()
    )
}
