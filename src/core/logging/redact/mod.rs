//! Secret masking for anything the backend logs (request/response bodies,
//! headers). Mirrors the desktop shell's rules so every source redacts the
//! same keys; the shell re-applies them as a backstop. Only credentials and
//! secrets are masked — customer data is kept on purpose.

mod logging_redact;
#[cfg(test)]
mod tests;

pub use logging_redact::REDACTED;
pub use logging_redact::body_for_log;
pub use logging_redact::is_sensitive_key;
pub use logging_redact::redact_json_text;
pub use logging_redact::redact_text;
pub use logging_redact::redact_value;
