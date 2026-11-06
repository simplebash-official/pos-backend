//! Condition checklist items for warranty claims. A shop maintains a list of conditions (e.g.
//! "Physical damage / cracks", "Liquid damage", "Screen cracked", "Seal broken") that can be
//! checked when receiving an item. Staff can choose from saved conditions or type a new one on
//! the fly, which gets persisted to the shop's list.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Starter fallbacks provided when a shop has no conditions in its database yet.
pub const DEFAULT_CONDITIONS: &[&str] = &[
    "Physical damage / cracks",
    "Liquid damage",
    "Seal broken",
    "Opened or tampered with",
];

/// The key of a condition with this name: `wcon_` and the first 12 hex digits of the SHA-256 of
/// the name lowercased without spaces. Derived, not random, so the same condition added on two
/// devices is one row (matching the convention of `repair_service_templates` and `cash_categories`).
#[must_use]
pub fn condition_template_key(name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(crate::domain::cash_categories::normalize_name(name).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!(
        "{}_{hex}",
        crate::core::constants::prefixes::WARRANTY_CONDITION
    )
}

/// A warranty claim condition checklist item as returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyCondition {
    /// `wcon_…`.
    pub key: String,
    /// Human description (e.g. "Physical damage / cracks", "Scratched lens").
    pub name: String,
    /// Whether this condition is active and offered for new claims.
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

/// `POST /warranty-claims/conditions`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateConditionRequest {
    pub name: String,
}

/// Response for `GET /warranty-claims/conditions`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WarrantyConditionsResponse {
    pub conditions: Vec<WarrantyCondition>,
}
