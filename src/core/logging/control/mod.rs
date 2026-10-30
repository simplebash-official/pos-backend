// Runtime logging control over stdin. The desktop shell owns each sidecar's
// stdin and writes one JSON line to change logging while the process runs:
//
//   {"cmd":"log_mode","enabled":true,"http_bodies":true,"sql":"slow"}
//
// This is what lets Settings → System Benchmark measure logging off vs on
// without restarting the services. Only active in `LOG_FORMAT=json` (desktop)
// mode: under Docker/web nothing reads stdin. Unknown or malformed lines are
// ignored, so a future shell can add fields safely.

mod apply_control;
mod control_line;
mod log_mode;
mod parse_control;
mod spawn_stdin_control;
#[cfg(test)]
mod tests;

pub use apply_control::apply_control;
pub use control_line::ControlLine;
pub use log_mode::LogMode;
pub use parse_control::parse_control;
pub use spawn_stdin_control::spawn_stdin_control;
