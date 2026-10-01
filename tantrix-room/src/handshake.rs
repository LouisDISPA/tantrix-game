//! The room socket handshake (`GET /room/{CODE}[?create=1]` with a
//! WebSocket upgrade), checked the same way by both servers before they
//! accept a socket. What happens on the socket is [`crate::engine`]'s job.

/// Why a room handshake is answered with an HTTP error instead of a socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The path does not hold a room code (400).
    InvalidCode,
    /// A page on another site opened the socket (403).
    CrossOrigin,
    /// `create=1` for a code that already has a room (409).
    CodeTaken,
    /// The room could not be read or created (500).
    StorageUnavailable,
}

impl Refusal {
    pub fn status(self) -> u16 {
        match self {
            Self::InvalidCode => 400,
            Self::CrossOrigin => 403,
            Self::CodeTaken => 409,
            Self::StorageUnavailable => 500,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidCode => "invalid room code",
            Self::CrossOrigin => "cross-origin room connection",
            Self::CodeTaken => "room code already exists",
            Self::StorageUnavailable => "room storage is unavailable",
        }
    }
}

/// Check a handshake for room `code` before looking the room up.
///
/// The code must be exactly a [`tantrix_proto::is_room_code`] code.
/// Browsers always send `Origin`; refusing other sites stops a foreign page
/// from opening a socket that rides on our player's cookie. A request
/// without one is not from a browser page, and has no cookie to abuse.
/// `expected_origin` is this server's origin as browsers see it, if known.
pub fn check(
    code: &str,
    origin: Option<&str>,
    expected_origin: Option<&str>,
) -> Result<(), Refusal> {
    if !tantrix_proto::is_room_code(code) {
        return Err(Refusal::InvalidCode);
    }
    match origin {
        Some(origin) if expected_origin != Some(origin) => Err(Refusal::CrossOrigin),
        _ => Ok(()),
    }
}

/// Whether the handshake's query string (without `?`) asks to create the
/// room. Only the creator's first connection does: reloads and reconnects
/// join, so an unknown code is reported instead of silently recreated.
pub fn wants_create(query: Option<&str>) -> bool {
    query.is_some_and(|query| query.split('&').any(|pair| pair == "create=1"))
}

#[cfg(test)]
mod tests {
    use super::{check, wants_create, Refusal};

    const HERE: Option<&str> = Some("http://localhost:8080");

    #[test]
    fn a_room_code_from_this_site_is_accepted() {
        assert_eq!(check("ABCDEF", HERE, HERE), Ok(()));
        // Not a browser page: there is no cookie to ride on.
        assert_eq!(check("ABCDEF", None, HERE), Ok(()));
    }

    #[test]
    fn bad_codes_and_other_sites_are_refused() {
        assert_eq!(check("abcdef", HERE, HERE), Err(Refusal::InvalidCode));
        assert_eq!(check("ABCDEFG", HERE, HERE), Err(Refusal::InvalidCode));
        assert_eq!(check("ABCDEI", HERE, HERE), Err(Refusal::InvalidCode));
        assert_eq!(
            check("ABCDEF", Some("http://evil.example"), HERE),
            Err(Refusal::CrossOrigin)
        );
        // An origin can't be checked against nothing: refuse it.
        assert_eq!(check("ABCDEF", HERE, None), Err(Refusal::CrossOrigin));
    }

    #[test]
    fn only_create_1_creates() {
        assert!(wants_create(Some("create=1")));
        assert!(wants_create(Some("x=2&create=1")));
        assert!(!wants_create(Some("create=0")));
        assert!(!wants_create(Some("recreate=1")));
        assert!(!wants_create(None));
    }
}
