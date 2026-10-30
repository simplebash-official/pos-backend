use super::token_buckets::TokenBuckets;

use std::time::Instant;

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

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|v: &f64| *v >= 0.0)
        .unwrap_or(default)
}

impl RequestLimits {
    #[must_use]
    pub fn new(general: (f64, f64), sensitive: (f64, f64), live: (f64, f64)) -> Self {
        Self {
            general: TokenBuckets::new(general.0, general.1),
            sensitive: TokenBuckets::new(sensitive.0, sensitive.1),
            live: TokenBuckets::new(live.0, live.1),
        }
    }

    /// Defaults, overridable with `RATE_LIMIT_BURST`, `RATE_LIMIT_REFILL_PER_SEC`,
    /// `RATE_LIMIT_SENSITIVE_BURST`, `RATE_LIMIT_SENSITIVE_REFILL_PER_SEC`,
    /// `RATE_LIMIT_LIVE_BURST` and `RATE_LIMIT_LIVE_REFILL_PER_SEC`.
    #[must_use]
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
            (
                env_f64("RATE_LIMIT_LIVE_BURST", 400.0),
                env_f64("RATE_LIMIT_LIVE_REFILL_PER_SEC", 100.0),
            ),
        )
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
