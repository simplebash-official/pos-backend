// Structured logging for the backend: subscriber setup, the per-request
// logging middleware, secret redaction and the `log_event!` macro for
// business events.
//
// Two output modes, picked by `LOG_FORMAT`:
// - `text` (default, web/Docker): the human-readable `fmt` output as before.
// - `json` (set by the Tauri desktop shell): one JSON object per line on
//   stdout with a local-offset timestamp, which the shell ingests into the
//   unified desktop activity log. Contract: `docs/logging.md` in the
//   compose repo.

pub mod control;
pub mod domain;
mod init;
mod log_settings;
mod macro_rules;
pub mod redact;
pub mod request;
mod settings;
mod sql_logging;

pub use init::init;
pub use log_settings::LogSettings;
pub use settings::settings;
pub use sql_logging::SqlLogging;
