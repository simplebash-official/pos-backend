//! In-process counters for brute-force protection. Owns the failure-window
//! bookkeeping only; callers decide what a "failure" is and what key it is
//! counted under (see `modules::auth::service::login`).
//!
//! Deliberately in memory, not in the database: a login burst must not turn
//! into a write burst, and every deployment runs one backend process per
//! database (desktop sidecar, single web container). A restart clears the
//! counters, which only ever errs towards letting a legitimate user back in.

mod core_rate_limit;
#[cfg(test)]
mod tests;

pub use core_rate_limit::FailureLimiter;
pub use core_rate_limit::LOGIN_FAILURES;
pub use core_rate_limit::RequestLimits;
pub use core_rate_limit::limit_requests;
