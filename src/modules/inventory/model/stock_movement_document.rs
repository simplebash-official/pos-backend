use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::inventory::{StockMovement, StockMovementType};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Mongo document shape representing a stock movement audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this stock movement record (e.g. stm_...).
    #[serde(default)]
    pub key: String,
    /// MongoDB ObjectId of the affected product.
    pub product_id: ObjectId,
    /// Key of the stock batch whose units moved; absent only on the
    /// audit-only return movements that move no quantity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_key: Option<String>,
    /// Quantity change (positive for additions, negative for reductions).
    pub quantity_delta: i64,
    /// Reason or classification for this movement (e.g. sale, intake, manual adjustment).
    pub movement_type: StockMovementType,
    /// External reference key (e.g. invoice key, purchase key, repair ticket key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
    /// Optional explanatory note or comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when this stock movement occurred.
    pub created_at: BsonDateTime,
    /// Timestamp when this movement record was last modified.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier of the client creating/updating this movement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl StockMovementDocument {
    #[must_use]
    pub fn into_stock_movement(self) -> StockMovement {
        let key = if self.key.is_empty() {
            generate_id(prefixes::STOCK_MOVEMENT)
        } else {
            self.key
        };
        StockMovement {
            id: self
                .id
                .expect("persisted stock movement document must have an id")
                .to_hex(),
            key,
            product_id: self.product_id.to_hex(),
            batch_key: self.batch_key,
            quantity_delta: self.quantity_delta,
            movement_type: self.movement_type,
            reference_id: self.reference_id,
            note: self.note,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
