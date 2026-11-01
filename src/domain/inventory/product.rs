//! Products, their requests, list queries and price tiers.

use crate::core::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// How a product's `barcode` was populated. `None` on `Product` whenever
/// `barcode` itself is `None` — a product may be created with no barcode at
/// all. A barcode set or corrected later via `PUT /products/{id}` is always
/// recorded as `Manual`, regardless of what it was before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BarcodeSource {
    Generated,
    Manual,
}

impl BarcodeSource {
    #[must_use]
    /// The name this value has on the wire and in storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Manual => "manual",
        }
    }
}

impl std::str::FromStr for BarcodeSource {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "generated" => Ok(Self::Generated),
            "manual" => Ok(Self::Manual),
            _ => Err(AppError::validation(format!("Invalid barcode source: {s}"))),
        }
    }
}

/// Body for `POST /products`. `sku` is deliberately absent — it's
/// generated server-side from the category/subcategory names resolved from
/// `category_key`/`subcategory_key` (see `service::sku::generate_sku`), not
/// client-supplied. `category_key`/`subcategory_key` must be the system-
/// generated `key` of an existing category/subcategory (see `GET
/// /categories` or `GET /categories/valid`) — never the display name.
/// A product carries no price of its own: every unit it starts with comes in
/// as a stock batch with its own cost and selling price — `openingStock` for
/// units on hand, or one purchase per `suppliers` entry. A product created
/// with neither has no stock and no price until its first purchase.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProductRequest {
    /// Optional manual barcode (omitted or null to skip).
    pub barcode: Option<String>,
    /// True to automatically generate a sequential EAN-13 barcode.
    #[serde(default)]
    pub auto_generate_barcode: bool,
    /// Name of the new product.
    pub name: String,
    /// Category key under which the product belongs.
    pub category_key: String,
    /// Subcategory key under which the product belongs.
    pub subcategory_key: String,
    /// Units on hand that did not come from a supplier purchase.
    pub opening_stock: Option<OpeningStockRequest>,
    /// Minimum stock alert threshold.
    #[serde(default)]
    pub min_stock_threshold: i64,
    /// Optional initial purchase deliveries from suppliers.
    #[serde(default)]
    pub suppliers: Vec<ProductSupplierIntake>,
    /// Whether this product's units are tracked individually by serial
    /// number. Once set, every purchase receipt and sale of this product
    /// must supply exact-count serial numbers (see `ProductSerial`).
    #[serde(default)]
    pub is_serialized: bool,
    /// Warranty length in months for units of this product, if any.
    pub warranty_months: Option<i64>,
}

/// Body for `DELETE /products` (batch delete). Any id that fails to parse
/// as an `ObjectId` is silently dropped rather than rejecting the whole
/// request — see `service::product::delete_products`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsRequest {
    /// List of product IDs or keys to delete.
    pub product_ids: Vec<String>,
}

/// Response for the batch-delete endpoint — count only, since the caller
/// already knows which ids it asked to delete.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsResponse {
    /// Number of products successfully deleted.
    pub deleted_count: u64,
}

/// Units a product is created with when they are not a supplier purchase.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpeningStockRequest {
    /// Units on hand.
    pub quantity: i64,
    /// Cost of one unit in cents.
    pub unit_cost_cents: i64,
    /// Price one unit sells at, in cents.
    pub selling_price_cents: i64,
}

/// Pagination metadata attached to any paginated list response.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    /// Current page number.
    pub page: u64,
    /// Items per page limit.
    pub limit: u64,
    /// Total records across all pages.
    pub total: u64,
    /// Total number of pages.
    pub total_pages: u64,
}

/// One sellable price of a product: every in-stock batch sharing this
/// `selling_price_cents`, merged. The cashier picks a tier, and the server
/// consumes that tier's batches oldest-first (see
/// `inventory::service::stock_batch::plan_tier_allocation`), so two
/// deliveries sold at the same price read as one option while each keeps its
/// own cost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PriceTier {
    /// Selling price of every unit in this tier, in cents.
    pub selling_price_cents: i64,
    /// Units left across the tier's batches.
    pub quantity: i64,
    /// Cheapest batch cost in the tier. Admin only (see `Product::without_cost`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_unit_cost_cents: Option<i64>,
    /// Dearest batch cost in the tier. Admin only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_unit_cost_cents: Option<i64>,
    /// When the oldest batch in the tier was received — the one a sale from
    /// this tier draws on first.
    pub oldest_received_at: DateTime<Utc>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_selling_price_cents: Option<i64>,
    /// Current count of items in stock (the sum of every batch's remaining
    /// quantity — derived from the stock-movement ledger).
    pub stock_quantity: i64,
    /// Threshold count for low stock warnings.
    pub min_stock_threshold: i64,
    /// True when individual units of this product are tracked by serial
    /// number (see `ProductSerial`) rather than only as an aggregate
    /// `stock_quantity` count.
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

/// Query params for `GET /products`. `sort_by` is whitelisted against a
/// fixed set of fields before it ever reaches a Mongo `sort` document (see
/// `service::product::sort_field_for`) rather than passed through directly.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ProductListQuery {
    /// Free-text search — matches product name, SKU, or barcode, plus any
    /// product whose category or subcategory name contains the term.
    pub search: Option<String>,
    /// Filter products by category key.
    pub category_key: Option<String>,
    /// Filter products by subcategory key.
    pub subcategory_key: Option<String>,
    /// Filter products below their min stock threshold.
    pub low_stock: Option<bool>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page.
    pub limit: Option<u64>,
    /// Field to sort by.
    pub sort_by: Option<String>,
    /// Sort order ("asc" or "desc").
    pub sort_order: Option<String>,
}

/// Response body for `GET /products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductListResponse {
    /// List of products for the requested page.
    pub items: Vec<Product>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}

/// An individual supplier intake batch submitted during product creation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSupplierIntake {
    /// Key of the supplier providing this stock.
    pub supplier_key: String,
    /// Number of units supplied.
    pub quantity: i64,
    /// Purchase unit cost in cents.
    pub cost_price_cents: i64,
    /// Price the units of this intake sell at, in cents.
    pub selling_price_cents: i64,
    /// Supplier invoice or shipment reference number.
    pub reference_no: Option<String>,
    /// Optional remarks or notes for this intake.
    pub notes: Option<String>,
}

/// Body for `PUT /products/{id}`. Every field is optional so a client can
/// send only what changed — `service::product::update_product` fills in
/// omitted fields from the existing document rather than clearing them.
/// `category_key`/`subcategory_key`, like on create, must be keys, not names.
/// `sku` stays immutable (no field here); `barcode` can be set/corrected once,
/// so a product created without the right barcode (or with an auto-generated
/// one) can be pointed at the real label number later.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProductRequest {
    /// Updated product name.
    pub name: Option<String>,
    /// Add or correct the scannable barcode (e.g. store the manufacturer
    /// barcode printed on the package). 8-14 digits; saved as a manual
    /// barcode. Omit to leave the existing barcode untouched — an empty
    /// string is a validation error, not a "clear".
    pub barcode: Option<String>,
    /// Updated category key.
    pub category_key: Option<String>,
    /// Updated subcategory key.
    pub subcategory_key: Option<String>,
    /// Updated low-stock warning threshold.
    pub min_stock_threshold: Option<i64>,
    /// Updated serialized-tracking flag.
    pub is_serialized: Option<bool>,
    /// Updated warranty length in months.
    pub warranty_months: Option<i64>,
}
