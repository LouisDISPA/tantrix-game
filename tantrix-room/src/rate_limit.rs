//! Per-client limits on the room HTTP endpoints, so a script can't scan
//! invitation codes or allocate rooms in bulk. Both servers key clients with
//! [`client_key`]; the native server counts with [`RateLimiter`], the Worker
//! with the runtime's rate-limit bindings configured to the same [`Limit`]s.

use std::collections::BTreeMap;
use std::net::IpAddr;

/// `requests` per `period_ms`, refilled evenly; a full allowance can be
/// spent at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub requests: u64,
    pub period_ms: u64,
}

/// `POST /api/rooms`: rooms a client may create.
pub const ROOM_CREATION: Limit = Limit {
    requests: 10,
    period_ms: 60_000,
};

/// `GET /api/rooms/code/{CODE}`: invitation codes a client may look up.
pub const CODE_LOOKUP: Limit = Limit {
    requests: 30,
    period_ms: 60_000,
};

/// The key a client's requests are counted under, or `None` for a loopback
/// client (local development and tests), which is never limited.
///
/// An IPv6 client is counted by its /64 network, which one host can
/// usually spread its requests over.
pub fn client_key(ip: IpAddr) -> Option<IpAddr> {
    let ip = ip.to_canonical();
    match ip {
        _ if ip.is_loopback() => None,
        IpAddr::V4(_) => Some(ip),
        IpAddr::V6(v6) => Some(IpAddr::V6((v6.to_bits() & !u128::from(u64::MAX)).into())),
    }
}

/// Counts requests per client key against one [`Limit`]: a token bucket per
/// key, stored as the time it is full again. Keys whose bucket is full are
/// forgotten, at most once per period, so memory follows recent clients.
/// Times are milliseconds from a monotonic clock.
#[derive(Debug)]
pub struct RateLimiter {
    limit: Limit,
    /// When each key's bucket is full again. Never at or before the last prune.
    full_at: BTreeMap<IpAddr, u64>,
    pruned_at: u64,
}

impl RateLimiter {
    pub fn new(limit: Limit) -> Self {
        assert!(limit.requests > 0 && limit.period_ms >= limit.requests);
        Self {
            limit,
            full_at: BTreeMap::new(),
            pruned_at: 0,
        }
    }

    /// Whether `key` may make one more request at `now`, counting it if so.
    /// Refused requests are not counted.
    pub fn allow(&mut self, key: IpAddr, now: u64) -> bool {
        if now >= self.pruned_at.saturating_add(self.limit.period_ms) {
            self.full_at.retain(|_, full_at| *full_at > now);
            self.pruned_at = now;
        }
        let cost = self.limit.period_ms / self.limit.requests;
        let full_at = self.full_at.get(&key).map_or(now, |&at| at.max(now));
        let after = full_at.saturating_add(cost);
        if after > now.saturating_add(self.limit.period_ms) {
            return false;
        }
        self.full_at.insert(key, after);
        true
    }

    /// How many keys are remembered.
    pub fn len(&self) -> usize {
        self.full_at.len()
    }

    pub fn is_empty(&self) -> bool {
        self.full_at.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    const A: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));
    const B: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2));

    fn allowed(limiter: &mut RateLimiter, key: IpAddr, now: u64, times: usize) -> usize {
        (0..times).filter(|_| limiter.allow(key, now)).count()
    }

    #[test]
    fn a_full_allowance_is_spent_at_once_then_refills_evenly() {
        let mut limiter = RateLimiter::new(ROOM_CREATION);
        assert_eq!(allowed(&mut limiter, A, 1_000, 11), 10);
        // One request comes back every 6 seconds.
        assert!(!limiter.allow(A, 6_999));
        assert!(limiter.allow(A, 7_000));
        assert!(!limiter.allow(A, 7_000));
        assert_eq!(allowed(&mut limiter, A, 61_000, 11), 9);
        // A whole idle period refills everything.
        assert_eq!(allowed(&mut limiter, A, 200_000, 11), 10);
    }

    #[test]
    fn clients_are_counted_apart() {
        let mut limiter = RateLimiter::new(CODE_LOOKUP);
        assert_eq!(allowed(&mut limiter, A, 0, 31), 30);
        assert_eq!(allowed(&mut limiter, B, 0, 31), 30);
        assert!(!limiter.allow(A, 1_999));
        assert!(limiter.allow(A, 2_000));
    }

    #[test]
    fn refused_requests_do_not_delay_the_refill() {
        let mut limiter = RateLimiter::new(ROOM_CREATION);
        assert_eq!(allowed(&mut limiter, A, 0, 10), 10);
        assert_eq!(allowed(&mut limiter, A, 5_000, 1_000), 0);
        assert!(limiter.allow(A, 6_000));
    }

    #[test]
    fn full_buckets_are_forgotten_once_per_period() {
        let mut limiter = RateLimiter::new(ROOM_CREATION);
        assert!(limiter.allow(A, 0));
        assert_eq!(allowed(&mut limiter, B, 50_000, 10), 10);
        assert_eq!(limiter.len(), 2);
        // A was full again at 6 s; B stays in debt until 110 s.
        assert!(limiter.allow(B, 60_000));
        assert_eq!(limiter.len(), 1);
        // No prune before another period has passed.
        assert!(limiter.allow(A, 119_999));
        assert_eq!(limiter.len(), 2);
        assert!(limiter.allow(B, 200_000));
        assert_eq!(limiter.len(), 1);
    }

    #[test]
    fn loopback_is_unlimited_and_ipv6_counts_by_network() {
        assert_eq!(client_key(Ipv4Addr::LOCALHOST.into()), None);
        assert_eq!(client_key(Ipv6Addr::LOCALHOST.into()), None);
        assert_eq!(
            client_key(Ipv4Addr::LOCALHOST.to_ipv6_mapped().into()),
            None
        );
        assert_eq!(client_key(A), Some(A));
        let IpAddr::V4(a) = A else { unreachable!() };
        assert_eq!(client_key(a.to_ipv6_mapped().into()), Some(A));
        let host: IpAddr = "2001:db8:1:2:3:4:5:6".parse().unwrap();
        let other_host: IpAddr = "2001:db8:1:2:ffff::1".parse().unwrap();
        let network: IpAddr = "2001:db8:1:2::".parse().unwrap();
        assert_eq!(client_key(host), Some(network));
        assert_eq!(client_key(other_host), Some(network));
        assert_ne!(
            client_key("2001:db8:1:3::1".parse().unwrap()),
            Some(network)
        );
    }
}
