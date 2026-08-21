// Mongo document shapes for the billing feature. Kept separate from
// `domain::billing` (the API-facing types) so BSON concerns like `ObjectId`
// never leak into request/response payloads. `items`/`split_payments`
// embed `domain::billing::InvoiceItem`/`SplitPayment` directly rather than
// duplicating near-identical Mongo-side structs — neither type holds a
// BSON-specific field (no `ObjectId`), so there's nothing a separate
// "document" shape would add.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::billing::{
        Invoice, InvoiceItem, PaymentRecord, ReturnItem, ReturnRecord, SplitPayment,
    },
};

/// Invoices are append-only (see `domain::billing::Invoice`'s module
/// comment, D3): every field here except `status`/`cancelled_*` is written
/// once at insert and never touched again.
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
    /// Current invoice status ("paid", "pending", "cancelled").
    pub status: String,
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
    /// Date and time when invoice was cancelled, if cancelled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<BsonDateTime>,
    /// User ID of staff member who cancelled the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_by: Option<String>,
    /// Explanation why this invoice was cancelled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
    /// Total amount in cents refunded against this invoice.
    #[serde(default)]
    pub refunded_cents: i64,
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
            notes: self.notes,
            shop_profile_snapshot: self.shop_profile_snapshot,
            warranty_terms_snapshot: self.warranty_terms_snapshot,
            document_selection: self.document_selection,
            cancelled_at: self.cancelled_at.map(|d| d.to_chrono()),
            cancelled_by: self.cancelled_by,
            cancellation_reason: self.cancellation_reason,
            refunded_cents: self.refunded_cents,
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

/// Type alias for return items embedded in return documents.
pub type ReturnItemDocument = ReturnItem;

/// Returns are append-only: every field is written once at insert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key for this return record (e.g. ret_...).
    #[serde(default)]
    pub key: String,
    /// Human-friendly sequential return number (e.g. RET-000001).
    pub return_number: String,
    /// Key of the invoice this return is applied to.
    pub invoice_key: String,
    /// Sequential number of the invoice returned against.
    pub invoice_number: String,
    /// Key of the customer linked to the original invoice, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Snapshot of customer's name at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    /// User ID of staff member who processed the return.
    pub cashier_id: String,
    /// Name snapshot of staff member who processed the return.
    pub cashier_name_snapshot: String,
    /// List of returned items.
    pub returned_items: Vec<ReturnItemDocument>,
    /// List of replacement or exchange items provided, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exchange_items: Vec<InvoiceItem>,
    /// Total monetary value of returned items in cents.
    pub return_subtotal_cents: i64,
    /// Total monetary value of replacement/exchange items in cents.
    pub exchange_subtotal_cents: i64,
    /// Net refund amount in cents (positive = cashback, negative = customer extra payment).
    pub net_refund_cents: i64,
    /// Payment method used for refund payout or difference collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_method: Option<String>,
    /// Key of payment record created for the refund or extra payment, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refund_payment_key: Option<String>,
    /// General notes or remarks for this return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Concurrency version number for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when return document was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when return document was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl ReturnDocument {
    pub fn into_return_record(self) -> ReturnRecord {
        let key = if self.key.is_empty() {
            generate_id(prefixes::RETURN)
        } else {
            self.key
        };
        ReturnRecord {
            id: self
                .id
                .expect("persisted return document must have an id")
                .to_hex(),
            key,
            return_number: self.return_number,
            invoice_key: self.invoice_key,
            invoice_number: self.invoice_number,
            customer_key: self.customer_key,
            customer_name_snapshot: self.customer_name_snapshot,
            cashier_id: self.cashier_id,
            cashier_name_snapshot: self.cashier_name_snapshot,
            returned_items: self.returned_items,
            exchange_items: self.exchange_items,
            return_subtotal_cents: self.return_subtotal_cents,
            exchange_subtotal_cents: self.exchange_subtotal_cents,
            net_refund_cents: self.net_refund_cents,
            payment_method: self.payment_method,
            refund_payment_key: self.refund_payment_key,
            notes: self.notes,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
