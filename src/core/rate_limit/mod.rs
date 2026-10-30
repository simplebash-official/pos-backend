// In-process counters for brute-force protection. Owns the failure-window
// bookkeeping only; callers decide what a "failure" is and what key it is
// counted under (see `modules::auth::service::login`).
//
// Deliberately in memory, not in the database: a login burst must not turn
// into a write burst, and every deployment runs one backend process per
// database (desktop sidecar, single web container). A restart clears the
// counters, which only ever errs towards letting a legitimate user back in.

mod failure_limiter;
mod limit_requests;
mod login_failures;
mod request_limits;
mod sweep_threshold;
#[cfg(test)]
mod tests;
mod token_buckets;

pub use failure_limiter::FailureLimiter;
pub use limit_requests::limit_requests;
pub use login_failures::LOGIN_FAILURES;
pub use request_limits::RequestLimits;
