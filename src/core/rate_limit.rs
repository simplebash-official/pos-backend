// In-process counters for brute-force protection. Owns the failure-window
// bookkeeping only; callers decide what a "failure" is and what key it is
// counted under (see `modules::auth::service::login`).
//
// Deliberately in memory, not in the database: a login burst must not turn
// into a write burst, and every deployment runs one backend process per
// database (desktop sidecar, single web container). A restart clears the
// counters, which only ever errs towards letting a legitimate user back in.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

/// Counts failures per key inside a fixed window and blocks a key once it
/// reaches `max_failures`, until that window has passed.
pub struct FailureLimiter {
    max_failures: u32,
    window: Duration,
    entries: Mutex<HashMap<String, (u32, Instant)>>,
}

/// Above this many tracked keys, expired entries are swept on the next write
/// so a stream of random keys can't grow the map without bound.
const SWEEP_THRESHOLD: usize = 10_000;

impl FailureLimiter {
    pub fn new(max_failures: u32, window: Duration) -> Self {
        Self {
            max_failures,
            window,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// `Err(retry_after)` while `key` is blocked; `Ok(())` otherwise.
    pub fn check(&self, key: &str) -> Result<(), Duration> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        match entries.get(key) {
            Some((count, started)) if *count >= self.max_failures => {
                let elapsed = started.elapsed();
                if elapsed < self.window {
                    Err(self.window - elapsed)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    /// Records one failure for `key`, starting a fresh window if the previous
    /// one has expired.
    pub fn record_failure(&self, key: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= SWEEP_THRESHOLD {
            let window = self.window;
            entries.retain(|_, (_, started)| started.elapsed() < window);
        }
        let now = Instant::now();
        let entry = entries.entry(key.to_string()).or_insert((0, now));
        if entry.1.elapsed() >= self.window {
            *entry = (0, now);
        }
        entry.0 = entry.0.saturating_add(1);
    }

    /// Forgets `key` (e.g. after a successful login).
    pub fn reset(&self, key: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }
}

/// Failed logins per shop + email: 10 within 15 minutes blocks that account's
/// login for the rest of the window. Keyed by account rather than client IP
/// because the IP headers behind the reverse proxy are client-controlled.
pub static LOGIN_FAILURES: LazyLock<FailureLimiter> =
    LazyLock::new(|| FailureLimiter::new(10, Duration::from_secs(15 * 60)));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_max_failures_and_reset_clears_it() {
        let limiter = FailureLimiter::new(3, Duration::from_secs(60));
        for _ in 0..2 {
            limiter.record_failure("k");
            assert!(limiter.check("k").is_ok());
        }
        limiter.record_failure("k");
        assert!(limiter.check("k").is_err());
        assert!(limiter.check("other").is_ok(), "keys are independent");

        limiter.reset("k");
        assert!(limiter.check("k").is_ok());
    }

    #[test]
    fn an_expired_window_unblocks() {
        let limiter = FailureLimiter::new(1, Duration::from_millis(20));
        limiter.record_failure("k");
        assert!(limiter.check("k").is_err());
        std::thread::sleep(Duration::from_millis(30));
        assert!(limiter.check("k").is_ok());
        // A failure after expiry starts a new window at 1, which (max 1)
        // blocks again — the count did not carry over from the old window.
        limiter.record_failure("k");
        assert!(limiter.check("k").is_err());
    }
}
