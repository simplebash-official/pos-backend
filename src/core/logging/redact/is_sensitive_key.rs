use regex::Regex;

use std::sync::OnceLock;

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
pub fn is_sensitive_key(key: &str) -> bool {
    sensitive_key_re().is_match(key)
}
