use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::billing::PaymentRecord;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

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
    /// Register session key this payment belongs to, if collected during an open session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
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
    #[must_use]
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
            session_key: self.session_key,
            recorded_by_user_id: self.recorded_by_user_id,
            recorded_by_name_snapshot: self.recorded_by_name_snapshot,
            recorded_at: self.recorded_at.to_chrono(),
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
