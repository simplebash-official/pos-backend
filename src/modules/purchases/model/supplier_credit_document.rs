use super::default_version::default_version;

use crate::domain::supplier_credits::{CreditReason, SupplierCredit};

use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A credit a supplier gave: an amount taken off what the shop owes. Immutable; a wrong credit
/// is voided by a reversing entry (`reverses_key`) whose key is derived from the original's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierCreditDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`scr_...`).
    pub key: String,
    pub supplier_key: String,
    /// The receipt this credit is against, when it is against one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_key: Option<String>,
    pub amount_cents: i64,
    pub reason: CreditReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Set on a voiding entry: the credit it cancels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_key: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
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

impl SupplierCreditDocument {
    /// `voided_by_key` is worked out by the service from the voiding entry.
    pub fn into_credit(self, voided_by_key: Option<String>) -> SupplierCredit {
        SupplierCredit {
            id: self
                .id
                .expect("persisted credit document must have an id")
                .to_hex(),
            key: self.key,
            supplier_key: self.supplier_key,
            receipt_key: self.receipt_key,
            amount_cents: self.amount_cents,
            reason: self.reason,
            note: self.note,
            reverses_key: self.reverses_key,
            voided_by_key,
            recorded_by_user_id: self.recorded_by_user_id,
            recorded_by_name_snapshot: self.recorded_by_name_snapshot,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
