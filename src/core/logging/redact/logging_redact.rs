use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

/// Bodies larger than this multiple of the cap are never parsed: only their
/// first `cap` bytes are scrubbed as text, so logging a multi-MB body costs
/// O(cap) instead of a full parse + re-serialize.
const PARSE_LIMIT_FACTOR: usize = 4;

/// Render a captured body for the log: JSON is redacted and re-serialized,
/// other text is secret-scrubbed; both are capped at `cap` bytes.
#[must_use]
pub fn body_for_log(bytes: &[u8], cap: usize) -> String {
    if bytes.len() > cap.saturating_mul(PARSE_LIMIT_FACTOR) {
        let mut end = cap.min(bytes.len());
        // Don't split a UTF-8 sequence.
        while end > 0 && end < bytes.len() && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
            end -= 1;
        }
        let prefix = String::from_utf8_lossy(&bytes[..end]);
        return format!(
            "{}…[truncated, {} bytes total]",
            redact_json_text(&prefix),
            bytes.len()
        );
    }
    let rendered = match serde_json::from_slice::<Value>(bytes) {
        Ok(mut json) => {
            redact_value(&mut json);
            json.to_string()
        }
        Err(_) => redact_text(&String::from_utf8_lossy(bytes)),
    };
    truncate(rendered, cap)
}

fn truncate(mut s: String, cap: usize) -> String {
    if s.len() <= cap {
        return s;
    }
    let mut cut = cap;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    s.push_str("…[truncated]");
    s
}

fn sensitive_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(password|passwd|pwd|secret|token|authorization|cookie|api[_-]?key|jwt|cvv|card_?number|otp|\bpin\b|^pin$|_pin$|pin_code|pincode)",
        )
        .expect("valid regex")
    })
}

#[must_use]
/// Whether a JSON key names a secret whose value must be masked.
pub fn is_sensitive_key(key: &str) -> bool {
    sensitive_key_re().is_match(key)
}

/// `"key": value` pairs in raw JSON text, for redacting a body too large to
/// parse. The key is checked with `is_sensitive_key`, same as the full rule.
fn json_pair_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#""((?:[^"\\]|\\.)*)"(\s*:\s*)("(?:[^"\\]|\\.)*"|-?\d[\d.eE+-]*|true|false)"#)
            .expect("valid regex")
    })
}

/// Secret-scrub raw JSON text without parsing it: masks the value of every
/// sensitive key and every secret-shaped string.
#[must_use]
pub fn redact_json_text(text: &str) -> String {
    let masked = json_pair_re().replace_all(text, |caps: &regex::Captures| {
        if is_sensitive_key(&caps[1]) {
            format!("\"{}\"{}\"{REDACTED}\"", &caps[1], &caps[2])
        } else {
            caps[0].to_string()
        }
    });
    redact_text(&masked)
}

#[must_use]
/// Masks secrets in free text such as a form-encoded body.
pub fn redact_text(text: &str) -> String {
    secret_value_re().replace_all(text, REDACTED).into_owned()
}

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

pub const REDACTED: &str = "[REDACTED]";

pub(super) fn secret_value_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(bearer\s+[A-Za-z0-9\-_\.=]+|eyJ[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_.+/=]*|\b[a-f0-9]{64}\b)",
        )
        .expect("valid regex")
    })
}
