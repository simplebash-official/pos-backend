/// The key of a regular bill, derived from its category and payee so the same bill made on two
/// offline devices is one row.
#[must_use]
pub fn recurring_expense_key(category_key: &str, payee_key: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{category_key}|{}", payee_key.unwrap_or("")).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{hex}",
        crate::core::constants::prefixes::RECURRING_EXPENSE
    )
}
