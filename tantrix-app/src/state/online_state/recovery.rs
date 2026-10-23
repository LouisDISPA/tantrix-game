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
    pub fn disconnected(&mut self, now: u64) -> Option<Retry> {
        if self
            .connected
            .take()
            .is_some_and(|since| now.saturating_sub(since) >= STABLE_CONNECTION)
        {
            *self = Self::default();
        }
        let end = *self.started.get_or_insert(now) + TOTAL_DEADLINE;
        let delay = *BACKOFF.get(self.attempts)?;
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
                recovery.disconnected(now),
                Some(Retry {
                    number,
                    start,
                    deadline
                })
            );
        }
        assert_eq!(recovery.disconnected(60_000), None);
    }

    #[test]
    fn suspension_cannot_extend_the_total_deadline() {
        let mut recovery = Recovery::default();
        recovery.disconnected(0).unwrap();
        assert_eq!(recovery.disconnected(60_000), None);
    }

    #[test]
    fn brief_connections_do_not_renew_the_allowance_but_stable_ones_do() {
        let mut recovery = Recovery::default();
        recovery.disconnected(0).unwrap();
        recovery.synchronized(2_000);
        assert_eq!(recovery.disconnected(3_000).unwrap().number, 2);
        recovery.synchronized(5_000);
        // Snapshot updates must not move the start of the stable period.
        recovery.synchronized(20_000);
        assert_eq!(recovery.disconnected(35_000).unwrap().number, 1);
    }

    #[test]
    fn manual_restart_renews_an_exhausted_cycle() {
        let mut recovery = Recovery::default();
        for now in [0, 2_000, 5_000] {
            recovery.disconnected(now).unwrap();
        }
        assert_eq!(recovery.disconnected(10_000), None);
        recovery = Recovery::default();
        assert_eq!(
            recovery.disconnected(10_000),
            Some(Retry {
                number: 1,
                start: 11_000,
                deadline: 31_000
            })
        );
    }
}
