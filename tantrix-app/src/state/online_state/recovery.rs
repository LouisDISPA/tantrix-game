//! Bounded recovery decisions. Times are milliseconds since the connection supervisor started.

const BACKOFF: [u64; 3] = [1_000, 2_000, 4_000];
const TOTAL_DEADLINE: u64 = 60_000;
const ATTEMPT_DEADLINE: u64 = 20_000;
const STABLE_CONNECTION: u64 = 30_000;

#[derive(Debug, Default)]
pub struct Recovery {
    started: Option<u64>,
    connected: Option<u64>,
    attempts: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Retry {
    pub number: usize,
    pub start: u64,
    pub deadline: u64,
}

impl Recovery {
    pub fn synchronized(&mut self, now: u64) {
        self.connected.get_or_insert(now);
    }

    /// Brief recoveries consume the existing allowance. Only a stable
    /// connection earns a new one; an explicit Retry creates a fresh Recovery.
    ///
    /// `jitter` ([`random_jitter`]) shortens the delay by up to half, so the
    /// clients a server restart dropped together don't all reconnect at the
    /// same instant. Delays only get shorter, so every bound still holds.
    pub fn disconnected(&mut self, now: u64, jitter: u16) -> Option<Retry> {
        if self
            .connected
            .take()
            .is_some_and(|since| now.saturating_sub(since) >= STABLE_CONNECTION)
        {
            *self = Self::default();
        }
        let end = *self.started.get_or_insert(now) + TOTAL_DEADLINE;
        let backoff = *BACKOFF.get(self.attempts)?;
        let delay = backoff - backoff / 2 * u64::from(jitter) / u64::from(u16::MAX);
        let start = now + delay;
        if start >= end {
            return None;
        }
        self.attempts += 1;
        Some(Retry {
            number: self.attempts,
            start,
            deadline: (start + ATTEMPT_DEADLINE).min(end),
        })
    }
}

/// A random [`Recovery::disconnected`] jitter, or none if the platform has
/// no randomness to give.
pub fn random_jitter() -> u16 {
    let mut bytes = [0; 2];
    getrandom::fill(&mut bytes).map_or(0, |()| u16::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_back_off_and_stop_at_three() {
        let mut recovery = Recovery::default();
        for (now, number, start, deadline) in [
            (0, 1, 1_000, 21_000),
            (21_000, 2, 23_000, 43_000),
            (43_000, 3, 47_000, 60_000),
        ] {
            assert_eq!(
                recovery.disconnected(now, 0),
                Some(Retry {
                    number,
                    start,
                    deadline
                })
            );
        }
        assert_eq!(recovery.disconnected(60_000, 0), None);
    }

    #[test]
    fn suspension_cannot_extend_the_total_deadline() {
        let mut recovery = Recovery::default();
        recovery.disconnected(0, 0).unwrap();
        assert_eq!(recovery.disconnected(60_000, 0), None);
    }

    #[test]
    fn brief_connections_do_not_renew_the_allowance_but_stable_ones_do() {
        let mut recovery = Recovery::default();
        recovery.disconnected(0, 0).unwrap();
        recovery.synchronized(2_000);
        assert_eq!(recovery.disconnected(3_000, 0).unwrap().number, 2);
        recovery.synchronized(5_000);
        // Snapshot updates must not move the start of the stable period.
        recovery.synchronized(20_000);
        assert_eq!(recovery.disconnected(35_000, 0).unwrap().number, 1);
    }

    #[test]
    fn manual_restart_renews_an_exhausted_cycle() {
        let mut recovery = Recovery::default();
        for now in [0, 2_000, 5_000] {
            recovery.disconnected(now, 0).unwrap();
        }
        assert_eq!(recovery.disconnected(10_000, 0), None);
        recovery = Recovery::default();
        assert_eq!(
            recovery.disconnected(10_000, 0),
            Some(Retry {
                number: 1,
                start: 11_000,
                deadline: 31_000
            })
        );
    }

    #[test]
    fn jitter_only_shortens_delays_by_up_to_half() {
        let first = |jitter| Recovery::default().disconnected(0, jitter).unwrap();
        assert_eq!(first(0).start, 1_000);
        assert!((750..=751).contains(&first(u16::MAX / 2).start));
        assert_eq!(first(u16::MAX).start, 500);

        // The most jitter keeps every bound: three attempts within the total.
        let mut recovery = Recovery::default();
        for (now, start, deadline) in [
            (0, 500, 20_500),
            (20_500, 21_500, 41_500),
            (41_500, 43_500, 60_000),
        ] {
            let retry = recovery.disconnected(now, u16::MAX).unwrap();
            assert_eq!((retry.start, retry.deadline), (start, deadline));
        }
        assert_eq!(recovery.disconnected(60_000, u16::MAX), None);
    }
}
