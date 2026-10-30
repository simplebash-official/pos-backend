use super::payment_kind::PaymentKind;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One payment, or part of one, counted against a receipt.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptSettlement {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`rset_...`).
    pub key: String,
    pub receipt_key: String,
    pub supplier_key: String,
    pub amount_cents: i64,
    pub payment_kind: PaymentKind,
    /// Key of the cash movement or expense payment that paid.
    pub payment_key: String,
    /// True once the payment was reversed; a reversed settlement no longer counts as paid.
    pub reversed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}
