//! Payments on an invoice and the requests that record them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The `payment{}` sub-object of `CreateSaleRequest` — every field that
/// describes how the sale was (or will be) paid for, grouped together
/// rather than scattered across the request's top level.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDetails {
    /// Selected payment method: `"cash"`, `"card"`, `"online"`, `"split"`, or `"credit"`.
    pub payment_method: String,
    /// Indicates if this transaction is recorded as unpaid credit.
    pub is_credit: bool,
    /// Total amount of money handed in by the customer in cents.
    pub amount_received_cents: Option<i64>,
    /// List of payment breakdown portions if payment method is "split".
    pub split_payments: Option<Vec<SplitPayment>>,
    /// Last 4 digits of payment card if paid by card.
    pub card_last4: Option<String>,
    /// Authorization code or reference number from card terminal.
    pub card_ref: Option<String>,
    /// Transaction ID or reference number for online payment.
    pub online_ref: Option<String>,
    /// Optional note or description for online transfer.
    pub online_note: Option<String>,
    /// Due date for credit invoice repayment (YYYY-MM-DD).
    pub due_date: Option<String>,
    /// How an up-front partial payment on a credit sale was taken:
    /// `"cash"` or `"card"`. Only meaningful when `is_credit` is true and
    /// `amount_received_cents` is a non-zero amount below the total — it
    /// selects the method recorded on the deposit's payment row. The
    /// invoice's own `payment_method` still stays `"credit"`.
    pub deposit_method: Option<String>,
}

/// Response payload containing all payments recorded against an invoice.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentListResponse {
    /// List of payments recorded against the invoice.
    pub payments: Vec<PaymentRecord>,
}

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
    pub version: i64,
}

/// Request body for recording an additional payment against an existing invoice.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordPaymentRequest {
    /// Repayment amount in cents.
    pub amount_cents: i64,
    /// Payment method used ("cash", "card", "online").
    pub payment_method: String,
    /// Optional reference note or description.
    pub notes: Option<String>,
    /// Register session key if taken during an open cashier shift.
    pub session_key: Option<String>,
}

/// One leg of a split payment. `method` is `"cash"` | `"card"` | `"online"`
/// (never `"split"` — that's the invoice-level `payment_method` value that
/// means "see `split_payments`").
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitPayment {
    /// Payment method used for this portion ("cash", "card", "online").
    pub method: String,
    /// Amount paid via this payment method in cents.
    pub amount_cents: i64,
    /// Last 4 digits of card number if paid by card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    /// Reference or transaction number for card or online payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}
