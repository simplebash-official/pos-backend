use super::default_version::default_version;
use crate::domain::purchase_receipts::{PaymentKind, ReceiptSettlement};

use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// One payment, or part of one, counted against a receipt. Immutable: a payment that is
/// reversed is not edited here, the settlement stops counting because its payment was reversed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptSettlementDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`rset_...`).
    pub key: String,
    /// Foreign key referencing the receipt that was paid.
    pub receipt_key: String,
    /// Foreign key referencing the supplier the receipt belongs to.
    pub supplier_key: String,
    pub amount_cents: i64,
    pub payment_kind: PaymentKind,
    /// Key of the cash movement or expense payment that paid.
    pub payment_key: String,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl ReceiptSettlementDocument {
    /// `reversed` is worked out by the service from the payment's own reversal.
    pub fn into_settlement(self, reversed: bool) -> ReceiptSettlement {
        ReceiptSettlement {
            id: self
                .id
                .expect("persisted settlement document must have an id")
                .to_hex(),
            key: self.key,
            receipt_key: self.receipt_key,
            supplier_key: self.supplier_key,
            amount_cents: self.amount_cents,
            payment_kind: self.payment_kind,
            payment_key: self.payment_key,
            reversed,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
