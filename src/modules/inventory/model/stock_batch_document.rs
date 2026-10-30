use super::default_version::default_version;

use crate::domain::inventory::{BatchSource, StockBatch};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Mongo document shape for one delivery of a product (`stock_batches`):
/// its own unit cost and selling price, and how many of its units are left.
/// `remaining_quantity` is a derived column — the sum of the stock movements
/// carrying this batch's key — so it is only ever moved together with a
/// movement (`service::stock::apply_stock_delta`) or recomputed from the
/// ledger by sync, never edited directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockBatchDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this batch (e.g. stb_...).
    pub key: String,
    /// Foreign key referencing the product.
    pub product_key: String,
    /// Where the units came from.
    pub source: BatchSource,
    /// The purchase that received this batch, when `source` is `purchase`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purchase_key: Option<String>,
    /// The supplier the units came from, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier_key: Option<String>,
    /// Units received into this batch.
    pub received_quantity: i64,
    /// Units still in stock (derived from the movement ledger).
    pub remaining_quantity: i64,
    /// Cost of one unit in cents.
    pub unit_cost_cents: i64,
    /// Price one unit sells at, in cents.
    pub selling_price_cents: i64,
    /// When the batch was received.
    pub received_at: BsonDateTime,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when the batch was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when the batch was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl StockBatchDocument {
    #[must_use]
    pub fn into_stock_batch(self) -> StockBatch {
        StockBatch {
            id: self
                .id
                .expect("persisted stock batch document must have an id")
                .to_hex(),
            key: self.key,
            product_key: self.product_key,
            source: self.source,
            purchase_key: self.purchase_key,
            supplier_key: self.supplier_key,
            received_quantity: self.received_quantity,
            remaining_quantity: self.remaining_quantity,
            unit_cost_cents: Some(self.unit_cost_cents),
            selling_price_cents: self.selling_price_cents,
            received_at: self.received_at.to_chrono(),
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
