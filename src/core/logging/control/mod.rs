//! Runtime logging control over stdin. The desktop shell owns each sidecar's
//! stdin and writes one JSON line to change logging while the process runs:
//!
//!   {"cmd":"log_mode","enabled":true,"http_bodies":true,"sql":"slow"}
//!
//! This is what lets Settings → System Benchmark measure logging off vs on
//! without restarting the services. Only active in `LOG_FORMAT=json` (desktop)
//! mode: under Docker/web nothing reads stdin. Unknown or malformed lines are
//! ignored, so a future shell can add fields safely.

mod logging_control;
#[cfg(test)]
mod tests;

pub use logging_control::ControlLine;
pub use logging_control::LogMode;
pub use logging_control::apply_control;
pub use logging_control::parse_control;
pub use logging_control::spawn_stdin_control;
