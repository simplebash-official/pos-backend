// Mongo document shapes for the billing feature. Kept separate from
// `domain::billing` (the API-facing types) so BSON concerns like `ObjectId`
// never leak into request/response payloads. `items`/`split_payments`
// embed `domain::billing::InvoiceItem`/`SplitPayment` directly rather than
// duplicating near-identical Mongo-side structs — neither type holds a
// BSON-specific field (no `ObjectId`), so there's nothing a separate
// "document" shape would add. Unit enums (`InvoiceStatus`, `ItemCondition`,
// `ItemDisposition`, `ReturnReason`, `CreditNoteStatus`) are likewise
// embedded directly — their serde `rename_all = "snake_case"` derive
// already produces the plain BSON string Mongo needs.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::billing::{
        CreditNote, CreditNoteItem, Invoice, InvoiceItem, InvoiceStatus, PaymentRecord,
        RefundBreakdownLeg, SplitPayment, compute_is_overdue,
    },
};

/// Invoices are append-only (see `domain::billing::Invoice`'s module
/// comment, D3): every field here except `status`/`voided_*`/`closed_*`/
/// `refunded_cents`/`credit_note_count` is written once at insert and never
/// touched again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key for this invoice (e.g. inv_...).
    #[serde(default)]
    pub key: String,
    /// Human-friendly sequential invoice number (e.g. INV-000001).
    pub invoice_number: String,
    /// Key of the customer linked to this invoice, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Snapshot of customer's name at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    /// Snapshot of customer's phone number at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone_snapshot: Option<String>,
    /// Snapshot of customer's address at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address_snapshot: Option<String>,
    /// Identifier of the cashier who processed this sale.
    pub cashier_id: String,
    /// Snapshot of the cashier's display name at the time of sale.
    pub cashier_name_snapshot: String,
    /// Line items included in this invoice.
    pub items: Vec<InvoiceItem>,
    /// Subtotal price before discounts in cents.
    pub subtotal_cents: i64,
    /// Type of discount applied ("percentage" or "fixed").
    #[serde(default = "default_discount_type")]
    pub discount_type: String,
    /// Discount value (percentage rate or fixed amount).
    #[serde(default)]
    pub discount_value: f64,
    /// Total computed discount amount in cents.
    pub discount_cents: i64,
    /// Final total invoice amount in cents.
    pub total_cents: i64,
    /// Payment method used ("cash", "card", "online", "split", "credit").
    pub payment_method: String,
    /// Breakdown of split payments if payment method is "split".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_payments: Option<Vec<SplitPayment>>,
    /// Whether this invoice was issued on credit.
    pub is_credit: bool,
    /// Total cash amount handed over by customer in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_received_cents: Option<i64>,
    /// Change due back to customer in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_due_cents: Option<i64>,
    /// Due date for credit invoice repayment (YYYY-MM-DD).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    /// Last 4 digits of credit/debit card, if card payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    /// Card transaction authorization reference number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<String>,
    /// Online payment transaction reference ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_ref: Option<String>,
    /// Additional notes for online payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_note: Option<String>,
    /// Current invoice lifecycle status.
    pub status: InvoiceStatus,
    /// General notes or remarks for this invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Frozen snapshot of shop profile info at the time of sale.
    pub shop_profile_snapshot: serde_json::Value,
    /// Frozen snapshot of warranty terms printed on invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_terms_snapshot: Option<String>,
    /// Selected document template or format style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_selection: Option<String>,
    /// Date and time when invoice was voided, if voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_at: Option<BsonDateTime>,
    /// User ID of staff member who voided the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by: Option<String>,
    /// Mandatory reason this invoice was voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_reason: Option<String>,
    /// Date and time when invoice was closed, if closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<BsonDateTime>,
    /// User ID of staff member who closed the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<String>,
    /// Total amount in cents refunded against this invoice.
    #[serde(default)]
    pub refunded_cents: i64,
    /// Count of credit notes (any status) recorded against this invoice.
    #[serde(default)]
    pub credit_note_count: i64,
    /// Concurrency version number for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when invoice was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when invoice was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

fn default_version() -> i64 {
    1
}

fn default_discount_type() -> String {
    "fixed".to_string()
}

impl InvoiceDocument {
    pub fn into_invoice(self) -> Invoice {
        let key = if self.key.is_empty() {
            generate_id(prefixes::INVOICE)
        } else {
            self.key
        };
        let is_overdue = compute_is_overdue(self.status, self.due_date.as_deref());
        Invoice {
            id: self
                .id
                .expect("persisted invoice document must have an id")
                .to_hex(),
            key,
            invoice_number: self.invoice_number,
            customer_key: self.customer_key,
            customer_name_snapshot: self.customer_name_snapshot,
            customer_phone_snapshot: self.customer_phone_snapshot,
            customer_address_snapshot: self.customer_address_snapshot,
            cashier_id: self.cashier_id,
            cashier_name_snapshot: self.cashier_name_snapshot,
            items: self.items,
            subtotal_cents: self.subtotal_cents,
            discount_type: self.discount_type,
            discount_value: self.discount_value,
            discount_cents: self.discount_cents,
            total_cents: self.total_cents,
            payment_method: self.payment_method,
            split_payments: self.split_payments,
            is_credit: self.is_credit,
            amount_received_cents: self.amount_received_cents,
            change_due_cents: self.change_due_cents,
            due_date: self.due_date,
            card_last4: self.card_last4,
            card_ref: self.card_ref,
            online_ref: self.online_ref,
            online_note: self.online_note,
            status: self.status,
            is_overdue,
            notes: self.notes,
            shop_profile_snapshot: self.shop_profile_snapshot,
            warranty_terms_snapshot: self.warranty_terms_snapshot,
            document_selection: self.document_selection,
            voided_at: self.voided_at.map(|d| d.to_chrono()),
            voided_by: self.voided_by,
            voided_reason: self.voided_reason,
            closed_at: self.closed_at.map(|d| d.to_chrono()),
            closed_by: self.closed_by,
            refunded_cents: self.refunded_cents,
            credit_note_count: self.credit_note_count,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}

/// Payments are append-only — no edit/delete endpoint exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key for this payment record (e.g. pay_...).
    #[serde(default)]
    pub key: String,
    /// Key of the invoice this payment is applied to.
    pub invoice_key: String,
    /// Amount paid in cents.
    pub amount_cents: i64,
    /// Payment method used ("cash", "card", "online").
    pub payment_method: String,
    /// Optional remarks or reference note for this payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// User ID of the staff member who collected this payment.
    pub recorded_by_user_id: String,
    /// Name snapshot of the staff member who collected this payment.
    pub recorded_by_name_snapshot: String,
    /// Timestamp when this payment occurred.
    pub recorded_at: BsonDateTime,
    /// Concurrency version number for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when payment document was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when payment document was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl PaymentDocument {
    pub fn into_payment_record(self) -> PaymentRecord {
        let key = if self.key.is_empty() {
            generate_id(prefixes::PAYMENT)
        } else {
            self.key
        };
        PaymentRecord {
            id: self
                .id
                .expect("persisted payment document must have an id")
                .to_hex(),
            key,
            invoice_key: self.invoice_key,
            amount_cents: self.amount_cents,
            payment_method: self.payment_method,
            notes: self.notes,
            recorded_by_user_id: self.recorded_by_user_id,
            recorded_by_name_snapshot: self.recorded_by_name_snapshot,
            recorded_at: self.recorded_at.to_chrono(),
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}

/// Type alias for credit note items embedded in credit note documents.
pub type CreditNoteItemDocument = CreditNoteItem;

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
            voided_at: self.voided_at.map(|d| d.to_chrono()),
            voided_by: self.voided_by,
            voided_reason: self.voided_reason,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
