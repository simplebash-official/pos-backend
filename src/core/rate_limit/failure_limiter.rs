use super::sweep_threshold::SWEEP_THRESHOLD;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Counts failures per key inside a fixed window and blocks a key once it
/// reaches `max_failures`, until that window has passed.
#[derive(Debug)]
pub struct FailureLimiter {
    pub(super) max_failures: u32,
    pub(super) window: Duration,
    pub(super) entries: Mutex<HashMap<String, (u32, Instant)>>,
}

impl FailureLimiter {
    #[must_use]
    pub fn new(max_failures: u32, window: Duration) -> Self {
        Self {
            max_failures,
            window,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// `Err(retry_after)` while `key` is blocked; `Ok(())` otherwise.
    pub fn check(&self, key: &str) -> Result<(), Duration> {
        let entries = self.entries.lock().expect("failure limiter mutex poisoned");
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
        let mut entries = self.entries.lock().expect("failure limiter mutex poisoned");
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
            .expect("failure limiter mutex poisoned")
            .remove(key);
    }
}
