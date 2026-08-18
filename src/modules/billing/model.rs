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
    domain::billing::{Invoice, InvoiceItem, PaymentRecord, SplitPayment},
};

/// Invoices are append-only (see `domain::billing::Invoice`'s module
/// comment, D3): every field here except `status`/`cancelled_*` is written
/// once at insert and never touched again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub invoice_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address_snapshot: Option<String>,
    pub cashier_id: String,
    pub cashier_name_snapshot: String,
    pub items: Vec<InvoiceItem>,
    pub subtotal_cents: i64,
    #[serde(default = "default_discount_type")]
    pub discount_type: String,
    #[serde(default)]
    pub discount_value: f64,
    pub discount_cents: i64,
    pub total_cents: i64,
    pub payment_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_payments: Option<Vec<SplitPayment>>,
    pub is_credit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_received_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_due_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_note: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub shop_profile_snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_terms_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_selection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<BsonDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
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
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}

/// Payments are append-only — no edit/delete endpoint exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub invoice_key: String,
    pub amount_cents: i64,
    pub payment_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    pub recorded_at: BsonDateTime,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
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
