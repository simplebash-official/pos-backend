use crate::core::constants::prefixes;

pub(super) fn opening_key(subject_key: &str) -> String {
    format!("{}_{subject_key}", prefixes::BALANCE_OPENING)
}
