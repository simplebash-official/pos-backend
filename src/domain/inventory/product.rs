use super::barcode_source::BarcodeSource;
use super::default_version::default_version;
use super::price_tier::PriceTier;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A product as returned to API clients. `id` (the Mongo `ObjectId` as a
/// hex string) is the stable route/lookup key; `key` is the human-shareable
/// prefixed id (see `core::id::generate_id`) — both are exposed since
/// different callers reference a product differently. `category`/
/// `category_key` (and their subcategory counterparts) are both present:
/// the `*_key` fields are the actual foreign keys stored on the product
/// (see `modules::inventory::model::ProductDocument`), while the bare
/// `category`/`subcategory` names are resolved at read time purely for
/// display so callers don't have to cross-reference `GET /categories`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key identifying this product (e.g. prd_...).
    pub key: String,
    /// Sequential SKU code (e.g. PHO-SCR-0001).
    pub sku: String,
    /// Scannable barcode string, if set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    /// Source origin of barcode ("generated" or "manual").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode_source: Option<BarcodeSource>,
    /// Product title or description.
    pub name: String,
    /// Key of parent category.
    pub category_key: String,
    /// Resolved display name of parent category.
    pub category: String,
    /// Key of subcategory.
    pub subcategory_key: String,
    /// Resolved display name of subcategory.
    pub subcategory: String,
    /// The prices this product can currently be sold at: one entry per
    /// distinct selling price among its stock batches that still hold units,
    /// cheapest first. Computed at read time from `stock_batches` (never
    /// stored), so it is a read-time enrichment on the sync wire as well.
    /// Empty when nothing is in stock.
    pub price_tiers: Vec<PriceTier>,
    /// Most recent unit selling price recorded on historical batches.
    /// Present even when current stock is 0. None only if product never had batches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_selling_price_cents: Option<i64>,
    /// Current count of items in stock (the sum of every batch's remaining
    /// quantity — derived from the stock-movement ledger).
    pub stock_quantity: i64,
    /// Threshold count for low stock warnings.
    pub min_stock_threshold: i64,
    /// True when individual units of this product are tracked by serial
    /// number (see `ProductSerial`) rather than only as an aggregate
    /// `stock_quantity` count.
    #[serde(default)]
    pub is_serialized: bool,
    /// Warranty length in months, snapshotted onto each `ProductSerial.
    /// warranty_months` at the moment a unit is sold (see
    /// `billing::service::sale::resolve_sale_item`). `None` means no
    /// warranty is offered on this product.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_months: Option<i64>,
    /// Timestamp when product was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when product was last modified.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// ID of client device that last updated this product.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl Product {
    /// The product as a caller who may not see what the shop paid for it gets it.
    #[must_use]
    pub fn without_cost(mut self) -> Self {
        for tier in &mut self.price_tiers {
            tier.min_unit_cost_cents = None;
            tier.max_unit_cost_cents = None;
        }
        self
    }
}
