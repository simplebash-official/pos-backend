use crate::core::config::RateLimits;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
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
    /// Blocks a key after `max_failures` failures within `window`.
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

/// Axum middleware: answers 429 once a client exhausts its bucket. The client
/// is nginx's `X-Real-IP` (it overwrites the header, so it can't be spoofed
/// through the proxy); a backend reached directly (desktop sidecar on
/// loopback) has none and shares one "local" bucket.
pub async fn limit_requests(
    axum::extract::State(limits): axum::extract::State<std::sync::Arc<RequestLimits>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let client = req
        .headers()
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local");
    if limits.allow(client, req.uri().path()) {
        next.run(req).await
    } else {
        crate::core::error::AppError::custom(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            crate::core::constants::codes::RATE_LIMITED,
            "Too many requests. Please slow down and try again shortly.",
        )
        .into_response()
    }
}

/// Failed logins per shop + username: 10 within 15 minutes blocks that account's
/// login for the rest of the window. Keyed by account rather than client IP
/// because the IP headers behind the reverse proxy are client-controlled.
pub static LOGIN_FAILURES: LazyLock<FailureLimiter> =
    LazyLock::new(|| FailureLimiter::new(10, Duration::from_secs(15 * 60)));

/// Per-client request limits for the whole API: a generous `general` bucket on
/// every route, and a tight `sensitive` one on the unauthenticated entry points
/// (`/api/auth`, `/api/system`, `/api/internal`) that a login-failure counter
/// alone doesn't cover (shop-code probing, setup, provisioning).
#[derive(Debug)]
pub struct RequestLimits {
    pub(super) general: TokenBuckets,
    pub(super) sensitive: TokenBuckets,
    /// Live calculations (`/api/calc`): one request per keystroke, so a bucket of their own that
    /// neither starves nor is starved by the rest of the API.
    pub(super) live: TokenBuckets,
}

impl RequestLimits {
    /// Limits as the configuration states them.
    #[must_use]
    pub fn new(limits: &RateLimits) -> Self {
        Self {
            general: TokenBuckets::new(limits.general.burst, limits.general.refill_per_second),
            sensitive: TokenBuckets::new(
                limits.sensitive.burst,
                limits.sensitive.refill_per_second,
            ),
            live: TokenBuckets::new(limits.live.burst, limits.live.refill_per_second),
        }
    }

    pub(super) fn allow(&self, client: &str, path: &str) -> bool {
        let now = Instant::now();
        if path.starts_with("/api/calc") {
            return self.live.take(client, now);
        }
        let sensitive = ["/api/auth", "/api/system", "/api/internal"]
            .iter()
            .any(|p| path.starts_with(p));
        self.general.take(client, now)
            && (!sensitive || self.sensitive.take(&format!("s:{client}"), now))
    }
}

/// Above this many tracked keys, expired entries are swept on the next write
/// so a stream of random keys can't grow the map without bound.
pub(super) const SWEEP_THRESHOLD: usize = 10_000;

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
