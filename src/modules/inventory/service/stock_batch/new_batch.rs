use crate::domain::inventory::{BatchSource, StockMovementType};

use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;

/// Everything needed to bring a new batch of units into stock.
#[derive(Debug)]
pub(crate) struct NewBatch {
    /// The product's `_id` (stock movements reference products by it).
    pub product_id: ObjectId,
    pub product_key: String,
    pub source: BatchSource,
    pub purchase_key: Option<String>,
    pub supplier_key: Option<String>,
    pub quantity: i64,
    pub unit_cost_cents: i64,
    pub selling_price_cents: i64,
    pub received_at: BsonDateTime,
    /// How the incoming movement is tagged (`PurchaseReceipt`, `OpeningBalance`, ...).
    pub movement_type: StockMovementType,
    pub reference_id: Option<String>,
    pub note: Option<String>,
}
