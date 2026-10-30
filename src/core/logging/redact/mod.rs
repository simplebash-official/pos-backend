// Secret masking for anything the backend logs (request/response bodies,
// headers). Mirrors the desktop shell's rules so every source redacts the
// same keys; the shell re-applies them as a backstop. Only credentials and
// secrets are masked — customer data is kept on purpose.

mod body_for_log;
mod is_sensitive_key;
mod redact_json_text;
mod redact_text;
mod redact_value;
mod redacted;
mod secret_value_re;
#[cfg(test)]
mod tests;

pub use body_for_log::body_for_log;
pub use is_sensitive_key::is_sensitive_key;
pub use redact_json_text::redact_json_text;
pub use redact_text::redact_text;
pub use redact_value::redact_value;
pub use redacted::REDACTED;
