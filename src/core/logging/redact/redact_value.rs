use super::is_sensitive_key::is_sensitive_key;
use super::redact_text::redact_text;
use super::redacted::REDACTED;
use super::secret_value_re::secret_value_re;

use serde_json::Value;

/// Recursively mask sensitive keys and secret-shaped strings in place.
pub fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if is_sensitive_key(key) && !v.is_null() {
                    *v = Value::String(REDACTED.into());
                } else {
                    redact_value(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_value),
        Value::String(s) if secret_value_re().is_match(s) => *s = redact_text(s),
        _ => {}
    }
}
