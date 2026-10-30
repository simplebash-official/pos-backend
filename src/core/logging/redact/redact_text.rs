use super::redacted::REDACTED;
use super::secret_value_re::secret_value_re;

#[must_use]
pub fn redact_text(text: &str) -> String {
    secret_value_re().replace_all(text, REDACTED).into_owned()
}
