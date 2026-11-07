//! Per-client limits on room creation and invitation lookups; the policy is
//! in [`tantrix_room::rate_limit`].

use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use axum::http::HeaderMap;
use tantrix_room::rate_limit::{self, RateLimiter};

/// Counts room requests per client in memory, so a restart forgets them.
pub(super) struct RoomLimits {
    started: Instant,
    creation: Mutex<RateLimiter>,
    lookup: Mutex<RateLimiter>,
}

impl RoomLimits {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            creation: Mutex::new(RateLimiter::new(rate_limit::ROOM_CREATION)),
            lookup: Mutex::new(RateLimiter::new(rate_limit::CODE_LOOKUP)),
        }
    }

    /// Whether `client` may create one more room now.
    pub(super) fn allow_creation(&self, client: IpAddr) -> bool {
        self.allow(&self.creation, client)
    }

    /// Whether `client` may look up one more invitation code now.
    pub(super) fn allow_lookup(&self, client: IpAddr) -> bool {
        self.allow(&self.lookup, client)
    }

    fn allow(&self, limiter: &Mutex<RateLimiter>, client: IpAddr) -> bool {
        let Some(key) = rate_limit::client_key(client) else {
            return true;
        };
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        limiter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .allow(key, now)
    }
}

/// The client's address: the socket's peer or, behind a trusted reverse
/// proxy, the address that proxy appended to `X-Forwarded-For`. Only the
/// last entry is the proxy's; the client may have sent the ones before it.
pub(super) fn client_ip(peer: IpAddr, headers: &HeaderMap, trust_forwarded: bool) -> IpAddr {
    if !trust_forwarded {
        return peer;
    }
    headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .and_then(|ip| ip.trim().parse().ok())
        .unwrap_or(peer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::net::Ipv4Addr;

    const PROXY: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    fn forwarded(values: &[&'static str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append("x-forwarded-for", HeaderValue::from_static(value));
        }
        headers
    }

    #[test]
    fn forwarded_addresses_count_only_behind_a_trusted_proxy() {
        let headers = forwarded(&["198.51.100.7, 203.0.113.9"]);
        assert_eq!(client_ip(PROXY, &headers, false), PROXY);
        assert_eq!(
            client_ip(PROXY, &headers, true),
            "203.0.113.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            client_ip(PROXY, &forwarded(&["198.51.100.7", "2001:db8::1"]), true),
            "2001:db8::1".parse::<IpAddr>().unwrap()
        );
        for headers in [forwarded(&[]), forwarded(&["203.0.113.9, unknown"])] {
            assert_eq!(client_ip(PROXY, &headers, true), PROXY);
        }
    }

    #[test]
    fn loopback_clients_are_not_limited() {
        let limits = RoomLimits::new();
        assert!((0..100).all(|_| limits.allow_creation(PROXY)));
        let client = "203.0.113.9".parse().unwrap();
        assert_eq!((0..100).filter(|_| limits.allow_lookup(client)).count(), 30);
    }
}
