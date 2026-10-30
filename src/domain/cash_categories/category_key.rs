use super::normalize_name::normalize_name;
use crate::domain::sessions::CashMovementType;

/// The key of the category named `name` under `movement_type`. Derived from the
/// two, not random, so the same name made on two offline devices (or seeded on a
/// desktop and in the cloud) is one row that sync merges instead of a duplicate.
#[must_use]
pub fn category_key(movement_type: CashMovementType, name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(normalize_name(name).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{}_{hex}",
        crate::core::constants::prefixes::CASH_CATEGORY,
        movement_type.as_str()
    )
}
