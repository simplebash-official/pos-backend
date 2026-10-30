use super::default_version::default_version;
use super::invoice_item::InvoiceItem;
use super::invoice_status::InvoiceStatus;
use super::split_payment::SplitPayment;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A completed or pending sale as returned to API clients. `id` is the
/// Mongo `ObjectId` hex string; `key` (`inv_...`) is the prefixed id other
/// modules/the frontend reference this invoice by. `shop_profile_snapshot`
/// is the frontend's full `ShopProfile` object, embedded verbatim at
/// creation time (D2) — reprinting this invoice's documents always uses
/// these frozen shop details, never "whatever the shop profile says today".
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key of the invoice (e.g. inv_...).
    pub key: String,
    /// Human-friendly sequential invoice number (e.g. INV-000001).
    pub invoice_number: String,
    /// Key of the customer if linked to a registered account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer's name at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    /// Customer's phone at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone_snapshot: Option<String>,
    /// Customer's address at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address_snapshot: Option<String>,
    /// Staff ID of cashier who processed the invoice.
    pub cashier_id: String,
    /// Cashier's display name at the time of sale.
    pub cashier_name_snapshot: String,
    /// Key of the active register session when invoice was issued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    /// Purchased line items and services.
    pub items: Vec<InvoiceItem>,
    /// Subtotal price before discounts in cents.
    pub subtotal_cents: i64,
    /// `"percentage"` | `"fixed"` — the discount type this invoice's
    /// `discountCents` was computed from (`"fixed"`/`0.0` when no discount
    /// was applied). Kept alongside the computed cents so a receipt can
    /// show "5% off" rather than just the resulting amount.
    pub discount_type: String,
    /// Numeric rate or flat value of discount applied.
    pub discount_value: f64,
    /// Discount amount deducted in cents.
    pub discount_cents: i64,
    /// Final total amount payable in cents.
    pub total_cents: i64,
    /// `"cash"` | `"card"` | `"online"` | `"split"` | `"credit"`.
    pub payment_method: String,
    /// Breakdown list of payments if split method used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_payments: Option<Vec<SplitPayment>>,
    /// True if sale was performed on credit/tab.
    pub is_credit: bool,
    /// Total amount received from customer in cash in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_received_cents: Option<i64>,
    /// Change given back to customer in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_due_cents: Option<i64>,
    /// Due date for payment if invoice is on credit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    /// Last 4 digits of card used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    /// Card transaction reference code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<String>,
    /// Online payment transaction reference code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_ref: Option<String>,
    /// Online payment note or bank reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_note: Option<String>,
    /// The invoice's current lifecycle state.
    pub status: InvoiceStatus,
    /// Whether `status`'s due date has passed while still `Pending`/
    /// `PartiallyPaid` — computed at read time, never stored.
    pub is_overdue: bool,
    /// Customer-facing or internal invoice notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Snapshot of shop branding and contact details when invoice was issued.
    pub shop_profile_snapshot: serde_json::Value,
    /// Warranty policy statement printed on receipt/invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_terms_snapshot: Option<String>,
    /// Receipt layout or document template format selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_selection: Option<String>,
    /// Time when the invoice was voided, if voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_at: Option<DateTime<Utc>>,
    /// ID of the user who voided the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by: Option<String>,
    /// Mandatory reason the invoice was voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_reason: Option<String>,
    /// Time when the invoice was closed, if closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    /// ID of the user who closed the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<String>,
    /// Total amount in cents refunded against this invoice from credit notes.
    #[serde(default)]
    pub refunded_cents: i64,
    /// Count of credit notes (any status) recorded against this invoice —
    /// display-only; `Closed`'s "no open credit note" guard re-queries the
    /// `credit_notes` collection rather than trusting this count alone.
    #[serde(default)]
    pub credit_note_count: i64,
    /// Timestamp when invoice was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when invoice was last updated.
    pub updated_at: DateTime<Utc>,
    /// Device identifier that created or last updated this invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
    /// Optimistic locking version number.
    #[serde(default = "default_version")]
    pub version: i64,
}
