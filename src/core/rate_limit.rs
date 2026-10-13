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

/// Failed logins per shop + username: 10 within 15 minutes blocks that account's
/// login for the rest of the window. Keyed by account rather than client IP
/// because the IP headers behind the reverse proxy are client-controlled.
pub static LOGIN_FAILURES: LazyLock<FailureLimiter> =
    LazyLock::new(|| FailureLimiter::new(10, Duration::from_secs(15 * 60)));

/// Token bucket per key: `burst` requests at once, refilled at `refill_per_sec`.
struct TokenBuckets {
    burst: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<String, (f64, Instant)>>,
}

impl TokenBuckets {
    fn new(burst: f64, refill_per_sec: f64) -> Self {
        Self {
            burst,
            refill_per_sec,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Takes one token for `key`; false when the bucket is empty.
    fn take(&self, key: &str, now: Instant) -> bool {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
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

/// Per-client request limits for the whole API: a generous `general` bucket on
/// every route, and a tight `sensitive` one on the unauthenticated entry points
/// (`/api/auth`, `/api/system`, `/api/internal`) that a login-failure counter
/// alone doesn't cover (shop-code probing, setup, provisioning).
pub struct RequestLimits {
    general: TokenBuckets,
    sensitive: TokenBuckets,
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|v: &f64| *v >= 0.0)
        .unwrap_or(default)
}

impl RequestLimits {
    pub fn new(general: (f64, f64), sensitive: (f64, f64)) -> Self {
        Self {
            general: TokenBuckets::new(general.0, general.1),
            sensitive: TokenBuckets::new(sensitive.0, sensitive.1),
        }
    }

    /// Defaults, overridable with `RATE_LIMIT_BURST`, `RATE_LIMIT_REFILL_PER_SEC`,
    /// `RATE_LIMIT_SENSITIVE_BURST` and `RATE_LIMIT_SENSITIVE_REFILL_PER_SEC`.
    pub fn from_env() -> Self {
        Self::new(
            (
                env_f64("RATE_LIMIT_BURST", 200.0),
                env_f64("RATE_LIMIT_REFILL_PER_SEC", 50.0),
            ),
            (
                env_f64("RATE_LIMIT_SENSITIVE_BURST", 20.0),
                env_f64("RATE_LIMIT_SENSITIVE_REFILL_PER_SEC", 0.5),
            ),
        )
    }

    fn allow(&self, client: &str, path: &str) -> bool {
        let now = Instant::now();
        let sensitive = ["/api/auth", "/api/system", "/api/internal"]
            .iter()
            .any(|p| path.starts_with(p));
        self.general.take(client, now)
            && (!sensitive || self.sensitive.take(&format!("s:{client}"), now))
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

    #[test]
    fn sensitive_paths_are_limited_harder_and_per_client() {
        let limits = RequestLimits::new((100.0, 0.0), (2.0, 0.0));
        assert!(limits.allow("a", "/api/auth/login"));
        assert!(limits.allow("a", "/api/auth/login"));
        assert!(
            !limits.allow("a", "/api/auth/login"),
            "sensitive burst spent"
        );
        assert!(
            limits.allow("a", "/api/billing"),
            "general routes unaffected"
        );
        assert!(
            limits.allow("b", "/api/auth/login"),
            "other clients unaffected"
        );
    }

    #[test]
    fn general_bucket_runs_dry_and_refills() {
        let b = TokenBuckets::new(2.0, 1.0);
        let t = Instant::now();
        assert!(b.take("k", t) && b.take("k", t));
        assert!(!b.take("k", t));
        assert!(b.take("k", t + Duration::from_secs(1)));
    }

    #[tokio::test]
    async fn middleware_answers_429_with_the_error_envelope() {
        use axum::{Router, body::Body, http::Request, routing::get};
        use tower::ServiceExt;

        let limits = std::sync::Arc::new(RequestLimits::new((100.0, 0.0), (1.0, 0.0)));
        let app = Router::new()
            .route("/api/auth/shop/x", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(limits, limit_requests));
        let call = |ip: &'static str| {
            let app = app.clone();
            async move {
                app.oneshot(
                    Request::builder()
                        .uri("/api/auth/shop/x")
                        .header("x-real-ip", ip)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
            }
        };
        assert_eq!(call("1.1.1.1").await.status(), 200);
        let blocked = call("1.1.1.1").await;
        assert_eq!(blocked.status(), 429);
        let body = axum::body::to_bytes(blocked.into_body(), 4096)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("RATE_LIMITED"));
        assert_eq!(call("2.2.2.2").await.status(), 200, "per-client buckets");
    }
}
