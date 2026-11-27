//! A shop's ready-made print services ("Design", "Mug printing"): a name, the usual price and
//! cost of one unit and the warranty that work comes with. The print form adds one as a line in
//! one tap, then the person adjusts it.

use crate::domain::warranties::ProductWarranty;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// The key of the service with this name: `pst_` and the first 12 hex digits of the SHA-256 of
/// the name lowercased without spaces. Derived, not random, so the same service added on two
/// offline tills is one row once they sync (the rule `cash_categories` follow).
#[must_use]
pub fn print_service_key(name: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(crate::domain::cash_categories::normalize_name(name).as_bytes());
    let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("{}_{hex}", crate::core::constants::prefixes::PRINT_SERVICE)
}

/// A print service as returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintService {
    pub id: String,
    /// `pst_…`.
    pub key: String,
    /// Unique per shop, ignoring case.
    pub name: String,
    /// The usual price of one unit of this work.
    pub unit_price_cents: i64,
    /// What one unit of this work usually costs the shop; absent when none is saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_cost_cents: Option<i64>,
    /// The cover this work usually comes with.
    pub warranty: ProductWarranty,
    /// A hidden service stays on old jobs but is not offered on new ones.
    pub active: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Body of `POST /print-services` and `PUT /print-services/{key}`: the whole service.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SavePrintServiceRequest {
    pub name: String,
    pub unit_price_cents: i64,
    /// Omit it to save no usual cost.
    pub unit_cost_cents: Option<i64>,
    pub warranty: ProductWarranty,
    pub active: bool,
}

/// Query of `GET /print-services`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PrintServiceListQuery {
    /// Also list hidden services (the settings screen); the print form leaves it out.
    pub include_hidden: Option<bool>,
}
