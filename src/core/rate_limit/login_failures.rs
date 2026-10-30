use super::failure_limiter::FailureLimiter;

use std::sync::LazyLock;
use std::time::Duration;

/// Failed logins per shop + username: 10 within 15 minutes blocks that account's
/// login for the rest of the window. Keyed by account rather than client IP
/// because the IP headers behind the reverse proxy are client-controlled.
pub static LOGIN_FAILURES: LazyLock<FailureLimiter> =
    LazyLock::new(|| FailureLimiter::new(10, Duration::from_secs(15 * 60)));
