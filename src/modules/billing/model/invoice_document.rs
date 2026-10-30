use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::billing::{
    Invoice, InvoiceItem, InvoiceStatus, SplitPayment, compute_is_overdue,
};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

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
    /// Register session key this invoice was billed in, if during an open cashier shift.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    /// Concurrency version number for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when invoice was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when invoice was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Device identifier that created or last updated this invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_discount_type() -> String {
    "fixed".to_string()
}

impl InvoiceDocument {
    #[must_use]
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
            session_key: self.session_key,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            updated_by_device: self.updated_by_device,
            version: self.version,
        }
    }
}
