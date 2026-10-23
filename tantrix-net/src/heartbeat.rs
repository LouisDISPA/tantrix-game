//! Keepalive timing shared by both transports. No intervals accumulate ticks.

pub(crate) const PING_INTERVAL: u64 = 30_000;
pub(crate) const PONG_TIMEOUT: u64 = 15_000;

pub(crate) struct Heartbeat {
    pub deadline: u64,
    awaiting_pong: bool,
    next_ping: u64,
}

impl Heartbeat {
    pub fn new(now: u64) -> Self {
        Self {
            deadline: now + PING_INTERVAL,
            awaiting_pong: false,
            next_ping: now + PING_INTERVAL,
        }
    }

    pub fn expired(&self, now: u64) -> bool {
        self.awaiting_pong && now >= self.deadline
    }

    pub fn ping(&mut self, now: u64) {
        self.awaiting_pong = true;
        self.next_ping = now + PING_INTERVAL;
        self.deadline = now + PONG_TIMEOUT;
    }

    pub fn pong(&mut self, now: u64) {
        if self.awaiting_pong {
            self.awaiting_pong = false;
            self.deadline = self.next_ping.max(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missed_pong_expires_even_after_suspension() {
        let mut heartbeat = Heartbeat::new(0);
        assert_eq!(heartbeat.deadline, 30_000);
        heartbeat.ping(30_000);
        assert!(!heartbeat.expired(44_999));
        assert!(heartbeat.expired(45_000));
        assert!(heartbeat.expired(120_000));
        // A suspended timer sends one ping on resume, never a burst.
        let mut suspended = Heartbeat::new(0);
        suspended.ping(120_000);
        assert_eq!(suspended.deadline, 135_000);
    }

    #[test]
    fn a_pong_keeps_the_thirty_second_ping_interval() {
        let mut heartbeat = Heartbeat::new(0);
        heartbeat.ping(30_000);
        heartbeat.pong(31_000);
        assert_eq!(heartbeat.deadline, 60_000);
        assert!(!heartbeat.expired(60_000));
    }
}
