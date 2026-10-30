use super::allocation::Allocation;
use super::billed_by::BilledBy;
use super::charge_kind::ChargeKind;
use super::payment_kind::PaymentKind;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// An extra charge as returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptCharge {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`rchg_...`).
    pub key: String,
    pub receipt_key: String,
    pub supplier_key: String,
    pub kind: ChargeKind,
    pub amount_cents: i64,
    pub billed_by: BilledBy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocation: Option<Allocation>,
    /// The payee a third-party charge was paid to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    /// What an `other` charge is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What paid a third-party charge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_kind: Option<PaymentKind>,
    /// Key of the cash movement or expense payment that paid a third-party charge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}
