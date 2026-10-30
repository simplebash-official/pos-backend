use super::credit_note_item_document::CreditNoteItemDocument;
use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::billing::{CreditNote, InvoiceItem, RefundBreakdownLeg};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Credit notes are append-only: every field is written once at insert,
/// except `status`/`voided_*` which `void_credit_note` sets once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditNoteDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key for this credit note (e.g. cn_...).
    #[serde(default)]
    pub key: String,
    /// Human-friendly sequential credit note number (e.g. CN-000001).
    pub credit_note_number: String,
    /// Key of the invoice this credit note is applied to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_key: Option<String>,
    /// Sequential number of the invoice returned against, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_number: Option<String>,
    /// True if this credit note has no linked invoice.
    #[serde(default)]
    pub no_receipt: bool,
    /// Key of the customer linked to the original invoice, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Snapshot of customer's name at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    /// User ID of staff member who processed the credit note.
    pub cashier_id: String,
    /// Name snapshot of staff member who processed the credit note.
    pub cashier_name_snapshot: String,
    /// List of returned items.
    pub returned_items: Vec<CreditNoteItemDocument>,
    /// List of replacement or exchange items provided, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exchange_items: Vec<InvoiceItem>,
    /// Set whenever `exchange_items` is non-empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange_reference: Option<String>,
    /// Total monetary value of returned items in cents.
    pub return_subtotal_cents: i64,
    /// Total monetary value of replacement/exchange items in cents.
    pub exchange_subtotal_cents: i64,
    /// Net refund amount in cents (positive = cashback, negative = customer
    /// extra payment) before the partial-payment cap.
    pub net_refund_cents: i64,
    /// The portion of `net_refund_cents` actually paid out/collected.
    #[serde(default)]
    pub refund_cash_cents: i64,
    /// The remainder applied as an invoice balance reduction instead of cash.
    #[serde(default)]
    pub balance_reduction_cents: i64,
    /// How `refund_cash_cents` was allocated across payment methods.
    #[serde(default)]
    pub refund_breakdown: Vec<RefundBreakdownLeg>,
    /// Keys of payment record(s) created for the refund or extra payment, if any.
    #[serde(default)]
    pub refund_payment_keys: Vec<String>,
    /// This credit note's resolution state.
    pub status: crate::domain::billing::CreditNoteStatus,
    /// True when approved past the normal return window, or no-receipt.
    #[serde(default)]
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
    /// Register session key this credit note belongs to, if during an open session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    /// Time when this credit note was voided, if voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_at: Option<BsonDateTime>,
    /// ID of the user who voided this credit note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by: Option<String>,
    /// Mandatory reason this credit note was voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_reason: Option<String>,
    /// Concurrency version number for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when credit note document was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when credit note document was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl CreditNoteDocument {
    #[must_use]
    pub fn into_credit_note(self) -> CreditNote {
        let key = if self.key.is_empty() {
            generate_id(prefixes::CREDIT_NOTE)
        } else {
            self.key
        };
        CreditNote {
            id: self
                .id
                .expect("persisted credit note document must have an id")
                .to_hex(),
            key,
            credit_note_number: self.credit_note_number,
            invoice_key: self.invoice_key,
            invoice_number: self.invoice_number,
            no_receipt: self.no_receipt,
            customer_key: self.customer_key,
            customer_name_snapshot: self.customer_name_snapshot,
            cashier_id: self.cashier_id,
            cashier_name_snapshot: self.cashier_name_snapshot,
            returned_items: self.returned_items,
            exchange_items: self.exchange_items,
            exchange_reference: self.exchange_reference,
            return_subtotal_cents: self.return_subtotal_cents,
            exchange_subtotal_cents: self.exchange_subtotal_cents,
            net_refund_cents: self.net_refund_cents,
            refund_cash_cents: self.refund_cash_cents,
            balance_reduction_cents: self.balance_reduction_cents,
            refund_breakdown: self.refund_breakdown,
            refund_payment_keys: self.refund_payment_keys,
            status: self.status,
            is_manager_override: self.is_manager_override,
            override_approved_by: self.override_approved_by,
            override_reason: self.override_reason,
            notes: self.notes,
            session_key: self.session_key,
            voided_at: self.voided_at.map(|d| d.to_chrono()),
            voided_by: self.voided_by,
            voided_reason: self.voided_reason,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
