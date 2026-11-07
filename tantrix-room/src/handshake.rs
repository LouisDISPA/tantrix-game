//! Shared checks of a room socket upgrade, run by both servers before a
//! socket reaches a room.

use tantrix_proto::{ONLINE_VERSION, ONLINE_VERSION_PARAM};

/// The protocol spoken by clients whose socket URL has no version. Every
/// release before the URL carried one speaks version 3; older releases join
/// `/room/{CODE}`, which is refused as an invalid ID. Once `ONLINE_VERSION`
/// moves past this, those clients are refused as out of date.
pub const UNVERSIONED_CLIENT: u32 = 3;

/// Why a socket upgrade is turned down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The URL names no well-formed game ID.
    InvalidCode,
    /// The room could not be loaded from storage.
    StorageUnavailable,
    /// A page on another site tried to open a socket with the visitor's
    /// session cookie.
    ForeignOrigin,
    /// The client speaks an older online protocol than this server.
    AppOutdated,
    /// The client speaks a newer online protocol than this server.
    ServerOutdated,
}

impl Refusal {
    /// The HTTP status when the upgrade is refused. Version refusals are
    /// answered on an accepted socket instead (see [`Self::closes_socket`]).
    pub fn status(self) -> u16 {
        match self {
            Self::InvalidCode => 400,
            Self::ForeignOrigin => 403,
            Self::AppOutdated | Self::ServerOutdated => 426,
            Self::StorageUnavailable => 503,
        }
    }

    /// Text for the player: the body of a refused upgrade, or the close
    /// reason when [`Self::closes_socket`].
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidCode => "invalid game ID",
            Self::StorageUnavailable => "room storage is unavailable",
            Self::ForeignOrigin => "cross-site room sockets are not allowed",
            Self::AppOutdated => "This app is out of date. Update or reload.",
            Self::ServerOutdated => "This server needs updating to work with your app.",
        }
    }

    /// Accept the socket, then close it with [`Self::message`] as its reason.
    /// Browsers hide the response to a refused upgrade, and clients show a
    /// close reason as their failure text, so players can read these.
    pub fn closes_socket(self) -> bool {
        matches!(self, Self::AppOutdated | Self::ServerOutdated)
    }
}

/// The page origin allowed to open room sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteOrigin<'a> {
    /// The site's public origin, such as `https://tantrix.example.com`.
    Exact(&'a str),
    /// Only the request's own `Host` is known (a server without a configured
    /// public origin, in local development). Any scheme matches.
    Host(&'a str),
}

impl SiteOrigin<'_> {
    fn allows(self, origin: &str) -> bool {
        match self {
            Self::Exact(site) => origin.eq_ignore_ascii_case(site),
            Self::Host(host) => {
                !host.is_empty()
                    && origin
                        .split_once("://")
                        .is_some_and(|(_, origin_host)| origin_host.eq_ignore_ascii_case(host))
            }
        }
    }
}

/// What a socket upgrade to `/room/{id}` carries.
#[derive(Debug, Clone, Copy)]
pub struct Upgrade<'a> {
    /// The room's game ID from the URL path, not yet checked.
    pub id: &'a str,
    /// The `Origin` header. Browsers always send one; the desktop and
    /// Android apps send none, so a missing origin is allowed.
    pub origin: Option<&'a str>,
    /// The page origin this server accepts.
    pub site: SiteOrigin<'a>,
    /// The URL's query string, which carries the client's online version.
    pub query: Option<&'a str>,
}

/// Whether `upgrade` may reach its room. The origin is checked first, so a
/// foreign page learns nothing about the room it named, then the game ID,
/// then the client's online version.
pub fn check(upgrade: &Upgrade) -> Result<(), Refusal> {
    if upgrade
        .origin
        .is_some_and(|origin| !upgrade.site.allows(origin))
    {
        return Err(Refusal::ForeignOrigin);
    }
    if !tantrix_proto::is_game_id(upgrade.id) {
        return Err(Refusal::InvalidCode);
    }
    match client_version(upgrade.query).map(|version| version.cmp(&ONLINE_VERSION)) {
        Some(std::cmp::Ordering::Equal) => Ok(()),
        Some(std::cmp::Ordering::Greater) => Err(Refusal::ServerOutdated),
        // An unreadable version is no protocol this server knows.
        Some(std::cmp::Ordering::Less) | None => Err(Refusal::AppOutdated),
    }
}

/// `None` when the URL names a version that isn't a number.
fn client_version(query: Option<&str>) -> Option<u32> {
    let value = query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .find_map(|pair| pair.strip_prefix(ONLINE_VERSION_PARAM)?.strip_prefix('='));
    match value {
        Some(value) => value.parse().ok(),
        None => Some(UNVERSIONED_CLIENT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "01234567-89ab-4def-8123-456789abcdef";
    const SITE: SiteOrigin = SiteOrigin::Exact("https://tantrix.example.com");

    fn current() -> String {
        format!("{ONLINE_VERSION_PARAM}={ONLINE_VERSION}")
    }

    fn upgrade<'a>(origin: Option<&'a str>, site: SiteOrigin<'a>, query: &'a str) -> Upgrade<'a> {
        Upgrade {
            id: ID,
            origin,
            site,
            query: Some(query),
        }
    }

    #[test]
    fn permanent_ids_are_validated_before_opening_a_room() {
        let query = current();
        assert_eq!(check(&upgrade(None, SITE, &query)), Ok(()));
        let code = Upgrade {
            id: "ABCDEF",
            ..upgrade(None, SITE, &query)
        };
        assert_eq!(check(&code), Err(Refusal::InvalidCode));
    }

    #[test]
    fn only_the_site_itself_and_native_apps_may_open_sockets() {
        let query = current();
        let local = SiteOrigin::Host("localhost:8080");
        for (origin, site) in [
            // Desktop and Android apps send no Origin.
            (None, SITE),
            (None, local),
            (Some("https://tantrix.example.com"), SITE),
            (Some("HTTPS://Tantrix.Example.com"), SITE),
            (Some("http://localhost:8080"), local),
            (Some("https://LOCALHOST:8080"), local),
        ] {
            assert_eq!(check(&upgrade(origin, site, &query)), Ok(()), "{origin:?}");
        }
        for (origin, site) in [
            ("https://evil.example", SITE),
            ("http://tantrix.example.com", SITE),
            ("https://tantrix.example.com:8443", SITE),
            ("https://tantrix.example.com.evil.example", SITE),
            ("http://localhost:8081", local),
            ("http://evil.example", local),
            // Sandboxed frames and local files.
            ("null", SITE),
            ("null", local),
            ("", local),
            // A request without a readable Host matches no page.
            ("http://", SiteOrigin::Host("")),
        ] {
            assert_eq!(
                check(&upgrade(Some(origin), site, &query)),
                Err(Refusal::ForeignOrigin),
                "{origin}"
            );
        }
        // A foreign page learns nothing about the room it named.
        let foreign = Upgrade {
            id: "ABCDEF",
            ..upgrade(Some("https://evil.example"), SITE, "v=0")
        };
        assert_eq!(check(&foreign), Err(Refusal::ForeignOrigin));
    }

    #[test]
    fn clients_must_speak_the_current_online_version() {
        let older = format!("{ONLINE_VERSION_PARAM}={}", ONLINE_VERSION - 1);
        let newer = format!("{ONLINE_VERSION_PARAM}={}", ONLINE_VERSION + 1);
        let mixed = format!("x=1&{}&y=2", current());
        for (query, expected) in [
            (current().as_str(), Ok(())),
            (mixed.as_str(), Ok(())),
            (older.as_str(), Err(Refusal::AppOutdated)),
            (newer.as_str(), Err(Refusal::ServerOutdated)),
            ("v=", Err(Refusal::AppOutdated)),
            ("v=three", Err(Refusal::AppOutdated)),
        ] {
            assert_eq!(check(&upgrade(None, SITE, query)), expected, "{query}");
        }
    }

    #[test]
    fn unversioned_clients_are_refused_only_once_the_protocol_moves_on() {
        let unversioned = |query| {
            check(&Upgrade {
                query,
                ..upgrade(None, SITE, "")
            })
        };
        let expected = if ONLINE_VERSION == UNVERSIONED_CLIENT {
            Ok(())
        } else {
            Err(Refusal::AppOutdated)
        };
        assert_eq!(unversioned(None), expected);
        assert_eq!(unversioned(Some("")), expected);
        assert_eq!(unversioned(Some("version=9&vv=9")), expected);
        const { assert!(UNVERSIONED_CLIENT <= ONLINE_VERSION) };
    }

    #[test]
    fn version_refusals_reach_the_player_as_a_close_reason() {
        for refusal in [Refusal::AppOutdated, Refusal::ServerOutdated] {
            assert!(refusal.closes_socket());
            // Close frames carry at most 123 bytes of reason.
            assert!(refusal.message().len() <= 123);
        }
        for refusal in [
            Refusal::InvalidCode,
            Refusal::StorageUnavailable,
            Refusal::ForeignOrigin,
        ] {
            assert!(!refusal.closes_socket());
        }
    }
}
