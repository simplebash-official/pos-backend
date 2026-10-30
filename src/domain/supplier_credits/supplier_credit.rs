use super::credit_reason::CreditReason;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A credit as returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierCredit {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`scr_...`).
    pub key: String,
    pub supplier_key: String,
    /// The receipt this credit is against, when it is against one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_key: Option<String>,
    pub amount_cents: i64,
    pub reason: CreditReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Set on a voiding entry: the credit it cancels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_key: Option<String>,
    /// Set when a voiding entry for this credit exists. Worked out when read, never stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by_key: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}
