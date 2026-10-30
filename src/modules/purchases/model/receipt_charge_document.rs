use super::default_version::default_version;
use crate::domain::purchase_receipts::{
    Allocation, BilledBy, ChargeKind, PaymentKind, ReceiptCharge,
};

use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// One extra charge on a receipt (shipping, duty, handling, insurance, other, or a supplier
/// discount). Immutable. A third-party charge also records the payment that paid it; a charge the
/// supplier billed is settled with the rest of the bill, so it carries no payment of its own.
/// `supplier_key` and `date` repeat the receipt's so the supplier balance can add a supplier's
/// charges up without loading every receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptChargeDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`rchg_...`).
    pub key: String,
    /// Foreign key referencing the receipt the charge is on.
    pub receipt_key: String,
    /// Foreign key referencing the receipt's supplier.
    pub supplier_key: String,
    /// The receipt's day.
    pub date: BsonDateTime,
    pub kind: ChargeKind,
    /// Always positive; the sign comes from `kind`.
    pub amount_cents: i64,
    pub billed_by: BilledBy,
    /// How the charge was spread over the lines; absent for a single-line receipt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocation: Option<Allocation>,
    /// The payee a third-party charge was paid to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    /// What an `other` charge is; absent for every other kind and for old rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_kind: Option<PaymentKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_key: Option<String>,
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

impl ReceiptChargeDocument {
    pub fn into_charge(self) -> ReceiptCharge {
        ReceiptCharge {
            id: self
                .id
                .expect("persisted charge document must have an id")
                .to_hex(),
            key: self.key,
            receipt_key: self.receipt_key,
            supplier_key: self.supplier_key,
            kind: self.kind,
            amount_cents: self.amount_cents,
            billed_by: self.billed_by,
            allocation: self.allocation,
            payee_key: self.payee_key,
            description: self.description,
            payment_kind: self.payment_kind,
            payment_key: self.payment_key,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
