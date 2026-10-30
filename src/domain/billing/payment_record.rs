use super::default_version::default_version;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A payment applied against an invoice — either recorded at sale-completion
/// time or later via `POST /billing/invoices/{key}/payments` (partial
/// credit repayment), or as a credit-note refund/extra-payment leg.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRecord {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key of the payment (e.g. pay_...).
    pub key: String,
    /// Key of the invoice this payment was credited towards.
    pub invoice_key: String,
    /// Payment amount in cents. Negative for a credit-note cash refund.
    pub amount_cents: i64,
    /// Payment method used ("cash", "card", "online").
    pub payment_method: String,
    /// Optional notes or remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Register session key this payment belongs to, if recorded during an open session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    /// User ID of employee who accepted the payment.
    pub recorded_by_user_id: String,
    /// Display name snapshot of employee who accepted payment.
    pub recorded_by_name_snapshot: String,
    /// Timestamp when payment occurred.
    pub recorded_at: DateTime<Utc>,
    /// Timestamp when payment record was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when payment record was last modified.
    pub updated_at: DateTime<Utc>,
    /// Version number for concurrency control.
    #[serde(default = "default_version")]
    pub version: i64,
}
