use super::product_summary::ProductSummary;
use super::supplier_summary::SupplierSummary;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A recorded stock intake as returned to API clients. `landedTotalCents` is
/// computed at read time (`lineTotalCents + chargesShareCents`), never stored — this
/// codebase never persists a derived numeric field that could go stale
/// (compare `LowStockItem.deficit`, also computed on the way out).
/// `supplier`/`product` are the enrichment the frontend's mock layer
/// currently builds client-side from separate calls.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this purchase (e.g. pur_...).
    pub key: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// Foreign key referencing the product.
    pub product_key: String,
    /// Number of units purchased and received.
    pub quantity: i64,
    /// What the line cost in cents, before extra charges: the supplier's price for these units.
    pub line_total_cents: i64,
    /// This line's share of the receipt's extra charges in cents (discounts negative), worked out
    /// once when the receipt was recorded.
    pub charges_share_cents: i64,
    /// What the units really cost: the line total plus its share of extra charges. Worked out on read.
    pub landed_total_cents: i64,
    /// The landed total per unit, rounded to the nearest cent for display; the stock itself is
    /// costed exactly (a lot that does not divide evenly is held as two batches one cent apart).
    pub landed_unit_cost_cents: i64,
    /// Price each received unit was set to sell at, in cents (the stock
    /// batch this purchase created starts at it; see `StockBatch`).
    pub selling_price_cents: i64,
    /// Purchase receipt date.
    pub date: DateTime<Utc>,
    /// Foreign key referencing the receipt (supplier invoice) this line belongs to.
    pub receipt_key: String,
    /// Enriched supplier details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<SupplierSummary>,
    /// Enriched product details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<ProductSummary>,
    /// Timestamp when purchase was recorded.
    pub created_at: DateTime<Utc>,
    /// Timestamp when purchase was last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Soft deletion timestamp if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}
