use super::batch_source::BatchSource;
use super::default_version::default_version;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One delivery of a product: its own cost, its own selling price and the
/// units of it still on the shelf. Every unit of stock belongs to exactly one
/// batch, and `remaining_quantity` is derived from the stock-movement ledger
/// (the sum of movements carrying this batch's key), exactly like
/// `Product.stock_quantity`, so concurrent offline sales from one batch
/// converge instead of overwriting each other.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockBatch {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key (e.g. stb_...).
    pub key: String,
    /// The product this batch belongs to.
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
    /// Cost of one unit in cents. Admin only (see `StockBatch::without_cost`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_cost_cents: Option<i64>,
    /// Price one unit of this batch sells at, in cents.
    pub selling_price_cents: i64,
    /// When the batch was received; sales from a tier draw on the oldest first.
    pub received_at: DateTime<Utc>,
    /// Timestamp when the batch was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the batch was last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// ID of client device that last updated this batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl StockBatch {
    /// The batch as a caller who may not see what the shop paid for it gets it.
    #[must_use]
    pub fn without_cost(mut self) -> Self {
        self.unit_cost_cents = None;
        self
    }
}
