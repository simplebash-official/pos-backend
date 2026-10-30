use super::sweep_threshold::SWEEP_THRESHOLD;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Token bucket per key: `burst` requests at once, refilled at `refill_per_sec`.
#[derive(Debug)]
pub(super) struct TokenBuckets {
    pub(super) burst: f64,
    pub(super) refill_per_sec: f64,
    pub(super) buckets: Mutex<HashMap<String, (f64, Instant)>>,
}

impl TokenBuckets {
    pub(super) fn new(burst: f64, refill_per_sec: f64) -> Self {
        Self {
            burst,
            refill_per_sec,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token for `key`; false when the bucket is empty.
    pub(super) fn take(&self, key: &str, now: Instant) -> bool {
        let mut buckets = self.buckets.lock().expect("token bucket mutex poisoned");
        // Bounded like `FailureLimiter`: drop buckets that have fully refilled.
        if buckets.len() >= SWEEP_THRESHOLD {
            let (burst, refill) = (self.burst, self.refill_per_sec);
            buckets.retain(|_, (tokens, at)| {
                *tokens + now.duration_since(*at).as_secs_f64() * refill < burst
            });
        }
        let entry = buckets.entry(key.to_string()).or_insert((self.burst, now));
        let elapsed = now.duration_since(entry.1).as_secs_f64();
        entry.0 = (entry.0 + elapsed * self.refill_per_sec).min(self.burst);
        entry.1 = now;
        if entry.0 >= 1.0 {
            entry.0 -= 1.0;
            true
        } else {
            false
        }
    }
}
