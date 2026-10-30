use super::normalize_name::normalize_name;
use super::payee_type::PayeeType;

/// The key of a stored payee, derived from its type and name for the same reason as
/// [`category_key`]: the same landlord typed on two offline devices is one row.
#[must_use]
pub fn payee_key(payee_type: PayeeType, name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(normalize_name(name).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{}_{hex}",
        crate::core::constants::prefixes::PAYEE,
        payee_type.as_str()
    )
}
