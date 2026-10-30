use super::is_sensitive_key::is_sensitive_key;
use super::redact_text::redact_text;
use super::redacted::REDACTED;
use regex::Regex;

use std::sync::OnceLock;

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
