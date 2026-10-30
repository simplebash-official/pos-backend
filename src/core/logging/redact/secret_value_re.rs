use regex::Regex;

use std::sync::OnceLock;

pub(super) fn secret_value_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(bearer\s+[A-Za-z0-9\-_\.=]+|eyJ[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_.+/=]*|\b[a-f0-9]{64}\b)",
        )
        .expect("valid regex")
    })
}
