//! Structured logging for the backend: subscriber setup, the per-request
//! logging middleware, secret redaction and the `log_event!` macro for
//! business events.
//!
//! Two output modes, picked by `LOG_FORMAT`:
//! - `text` (web/Docker): the human-readable `fmt` output.
//! - `json` (set by the Tauri desktop shell): one JSON object per line on
//!   stdout with a local-offset timestamp, which the shell ingests into the
//!   unified desktop activity log. Contract: `docs/logging.md` in the
//!   compose repo.

pub mod control;
mod core_logging;
pub mod domain;
mod macro_rules;
pub mod redact;
pub mod request;

pub use core_logging::LogSettings;
pub use core_logging::SqlLogging;
pub use core_logging::init;
pub use core_logging::{install, settings};
