use super::credit_note_item::CreditNoteItem;
use super::credit_note_status::CreditNoteStatus;
use super::default_version::default_version;
use super::invoice_item::InvoiceItem;
use super::refund_breakdown_leg::RefundBreakdownLeg;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A return/refund/exchange transaction record.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreditNote {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key of the credit note (e.g. cn_...).
    pub key: String,
    /// Human-friendly sequential credit note number (e.g. CN-000001).
    pub credit_note_number: String,
    /// Unique business key of the associated invoice, if any (absent for a
    /// no-receipt credit note).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_key: Option<String>,
    /// Human-friendly invoice number, if `invoiceKey` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_number: Option<String>,
    /// True if this credit note has no linked invoice.
    pub no_receipt: bool,
    /// Key of the customer linked to the original invoice, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer's name at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    /// Staff ID of cashier who processed the credit note.
    pub cashier_id: String,
    /// Cashier's display name at the time of the credit note.
    pub cashier_name_snapshot: String,
    /// List of returned items.
    pub returned_items: Vec<CreditNoteItem>,
    /// List of replacement or exchange items provided, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exchange_items: Vec<InvoiceItem>,
    /// Set whenever `exchangeItems` is non-empty — lets reporting
    /// distinguish a genuine return (money leaving, nothing replacing it)
    /// from an exchange (product swapped, net-neutral or near-neutral).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange_reference: Option<String>,
    /// Total monetary value of returned items in cents.
    pub return_subtotal_cents: i64,
    /// Total monetary value of replacement/exchange items in cents.
    pub exchange_subtotal_cents: i64,
    /// Net refund amount in cents (positive = cashback to customer,
    /// negative = customer pays extra) before the partial-payment cap.
    pub net_refund_cents: i64,
    /// The portion of `netRefundCents` actually paid out (or collected, if
    /// negative) in cash/card/online — capped at what was actually paid on
    /// the invoice so far. Equal to `netRefundCents` whenever no cap
    /// applies (no invoice, or the invoice was fully paid).
    pub refund_cash_cents: i64,
    /// The remainder of a positive `netRefundCents` that couldn't be paid
    /// out in cash because the invoice hadn't been paid that much yet —
    /// reduces what the invoice still owes instead of being handed over as
    /// cash the shop never received.
    pub balance_reduction_cents: i64,
    /// How `refundCashCents` was allocated across payment methods.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refund_breakdown: Vec<RefundBreakdownLeg>,
    /// Keys of payment record(s) created for the refund or extra payment, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refund_payment_keys: Vec<String>,
    /// This credit note's resolution state.
    pub status: CreditNoteStatus,
    /// True when this credit note was approved past the normal return
    /// window, or as a no-receipt return.
    pub is_manager_override: bool,
    /// User ID of the Admin who approved the override, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_approved_by: Option<String>,
    /// The reason given for the override, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_reason: Option<String>,
    /// General notes or remarks for this credit note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Register session key this credit note belongs to, if processed during an open session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    /// Time when this credit note was voided, if voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_at: Option<DateTime<Utc>>,
    /// ID of the user who voided this credit note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by: Option<String>,
    /// Mandatory reason this credit note was voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_reason: Option<String>,
    /// Timestamp when the credit note was recorded.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the credit note was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    #[serde(default = "default_version")]
    pub version: i64,
}
