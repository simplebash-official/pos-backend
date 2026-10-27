// Pure business types for the inventory feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::inventory::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::core::error::AppError;

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
    pub fn without_cost(mut self) -> Self {
        for tier in &mut self.price_tiers {
            tier.min_unit_cost_cents = None;
            tier.max_unit_cost_cents = None;
        }
        self
    }
}

fn default_version() -> i64 {
    1
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

/// Where a stock batch's units came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchSource {
    /// A supplier purchase receipt (`purchase_key` is set).
    Purchase,
    /// Stock a product was created with, or held before batches existed.
    Opening,
    /// A spreadsheet import row.
    Import,
    /// A manual stock-in adjustment.
    Adjustment,
}

impl BatchSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Purchase => "purchase",
            Self::Opening => "opening",
            Self::Import => "import",
            Self::Adjustment => "adjustment",
        }
    }
}

impl std::str::FromStr for BatchSource {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "purchase" => Ok(Self::Purchase),
            "opening" => Ok(Self::Opening),
            "import" => Ok(Self::Import),
            "adjustment" => Ok(Self::Adjustment),
            _ => Err(AppError::validation(format!("Invalid batch source: {s}"))),
        }
    }
}

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
    pub fn without_cost(mut self) -> Self {
        self.unit_cost_cents = None;
        self
    }
}

/// How many units of one sale/return line came out of (or went back into)
/// one stock batch, and what each of those units cost. A line at one price
/// tier can span several batches (oldest first), so an invoice line carries
/// a list of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchAllocation {
    /// The batch the units came from.
    pub batch_key: String,
    /// Units taken from that batch.
    pub quantity: i64,
    /// Cost of one of those units, in cents (the batch's cost at sale time).
    pub unit_cost_cents: i64,
}

/// Response for `GET /products/{key}/batches`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockBatchListResponse {
    /// Every batch of the product, newest first (empty ones included, so the
    /// intake history stays visible).
    pub items: Vec<StockBatch>,
}

/// Body for `PATCH /batches/{key}`. Either field may be sent alone. Changing
/// the cost is Admin-only (the handler checks it).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStockBatchRequest {
    /// New selling price for the batch's remaining units, in cents.
    pub selling_price_cents: Option<i64>,
    /// Corrected unit cost, in cents (Admin only).
    pub unit_cost_cents: Option<i64>,
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
    #[serde(default)]
    pub reference_no: Option<String>,
    /// Optional remarks or notes for this intake.
    #[serde(default)]
    pub notes: Option<String>,
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
    #[serde(default)]
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
    #[serde(default)]
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
    #[serde(default)]
    pub warranty_months: Option<i64>,
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

/// Query params for `GET /overview`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct InventoryOverviewQuery {
    /// Free-text search — matches product name, SKU, or barcode, plus any
    /// product whose category or subcategory name contains the term.
    pub search: Option<String>,
    /// Filter by category key.
    pub category_key: Option<String>,
    /// Filter by subcategory key.
    pub subcategory_key: Option<String>,
    /// Filter only low stock items.
    pub low_stock: Option<bool>,
    /// Page number.
    pub page: Option<u64>,
    /// Page size.
    pub limit: Option<u64>,
    /// Sort field.
    pub sort_by: Option<String>,
    /// Sort direction.
    pub sort_order: Option<String>,
}

/// Top-level inventory metrics for the dashboard header.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryMetrics {
    /// Total count of all products in catalog.
    pub total_items: u64,
    /// Total count of categories.
    pub total_categories: u64,
    /// Total count of subcategories.
    pub total_subcategories: u64,
    /// Total number of products currently below low stock threshold.
    pub low_stock_alerts: u64,
}

/// Subcategory accordion section containing its matching products list and pagination meta.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventorySubcategoryOverview {
    /// Subcategory key.
    pub key: String,
    /// Parent category key.
    pub category_key: String,
    /// Subcategory name.
    pub name: String,
    /// Total product count in this subcategory.
    pub total_items: u64,
    /// List of products on the current page.
    pub products: Vec<Product>,
    /// Pagination details for this subcategory.
    pub pagination: PaginationMeta,
}

/// Category accordion section containing nested subcategories and aggregate counts.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryCategoryOverview {
    /// Category key.
    pub key: String,
    /// Category name.
    pub name: String,
    /// Category icon.
    pub icon: String,
    /// Category color code.
    pub color: String,
    /// Total items across all subcategories in this category.
    pub total_items: u64,
    /// Count of subcategories under this category.
    pub subcategories_count: u64,
    /// Overview list of nested subcategories.
    pub subcategories: Vec<InventorySubcategoryOverview>,
}

/// Response payload for `GET /api/inventory/overview`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOverviewResponse {
    /// Summary inventory metrics.
    pub metrics: InventoryMetrics,
    /// Grouped category hierarchical data.
    pub categories: Vec<InventoryCategoryOverview>,
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

/// Response body for `GET /products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductListResponse {
    /// List of products for the requested page.
    pub items: Vec<Product>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
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

/// Body for `PATCH /products/{id}/stock`. `delta` is signed rather than an
/// absolute target quantity, so concurrent adjustments compose instead of
/// racing to overwrite each other's target value. Stock always lives in a
/// batch, so a negative delta names the batch it is taken from (`batchKey`),
/// and a positive delta brings in a new `adjustment` batch, which needs its
/// own cost and selling price.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockAdjustmentRequest {
    /// Signed integer change in stock quantity (+/-).
    pub delta: i64,
    /// Reason or note for manual stock adjustment.
    #[serde(default)]
    pub reason: Option<String>,
    /// The batch a negative delta is taken from.
    #[serde(default)]
    pub batch_key: Option<String>,
    /// Unit cost of the units a positive delta brings in, in cents.
    #[serde(default)]
    pub unit_cost_cents: Option<i64>,
    /// Selling price of the units a positive delta brings in, in cents.
    #[serde(default)]
    pub selling_price_cents: Option<i64>,
}

/// Response for a stock adjustment — includes both the before/after
/// quantities so a client can display the change without a second request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockAdjustmentResponse {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// Product SKU code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// New stock level after adjustment.
    pub stock_quantity: i64,
    /// Stock level prior to adjustment.
    pub previous_stock_quantity: i64,
    /// Quantity difference applied (+/-).
    pub delta: i64,
    /// Timestamp of adjustment.
    pub updated_at: DateTime<Utc>,
}

/// One entry in the low-stock report. `deficit` (threshold minus current
/// quantity) is precomputed here so clients don't each reimplement the
/// subtraction.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LowStockItem {
    /// Product MongoDB hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// Product SKU code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// Current stock on hand.
    pub stock_quantity: i64,
    /// Threshold configured for alerts.
    pub min_stock_threshold: i64,
    /// Units below threshold (min_stock_threshold - stock_quantity).
    pub deficit: i64,
}

/// Response for `GET /products/low-stock`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LowStockResponse {
    /// List of products requiring restocking.
    pub items: Vec<LowStockItem>,
    /// Total count of low stock items.
    pub total: u64,
}

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

/// Why a stock movement happened. Every adjustment (manual or automated)
/// gets recorded as a `StockMovement` tagged with one of these, so the
/// movement history stays auditable even once the originating request is
/// long gone. `Sale`/`PurchaseReceipt`/`ManualAdjustment` are in active use;
/// `RepairPartConsumption` is reserved for when repairs start consuming
/// parts directly. `InvoiceVoidReversal` and the three `Return*` variants
/// used to be a single overloaded `Return` variant — split apart so an
/// invoice-void stock restoration is distinguishable in the audit trail
/// from a credit-note-driven restock/write-off/supplier-RMA.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementType {
    Sale,
    PurchaseReceipt,
    RepairPartConsumption,
    ManualAdjustment,
    /// Stock restored because an invoice was voided (`billing::service::sale::void_invoice`).
    InvoiceVoidReversal,
    /// A credit-note line in `Resalable`/`OpenBoxDiscount` condition put back into sellable stock.
    ReturnRestock,
    /// A credit-note line in `Damaged` condition with disposition `WriteOffScrap` — audit-only, no quantity change.
    ReturnWriteOff,
    /// A credit-note line in `Damaged` condition with disposition `ReturnToSupplier` — audit-only, no quantity change.
    ReturnSupplierRma,
    /// Stock a product started with (or that predates the ledger). Sync treats
    /// stock as the sum of movements, so every unit needs a movement behind it.
    OpeningBalance,
}

impl StockMovementType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sale => "sale",
            Self::PurchaseReceipt => "purchase_receipt",
            Self::RepairPartConsumption => "repair_part_consumption",
            Self::ManualAdjustment => "manual_adjustment",
            Self::InvoiceVoidReversal => "invoice_void_reversal",
            Self::ReturnRestock => "return_restock",
            Self::ReturnWriteOff => "return_write_off",
            Self::ReturnSupplierRma => "return_supplier_rma",
            Self::OpeningBalance => "opening_balance",
        }
    }
}

impl std::str::FromStr for StockMovementType {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sale" => Ok(Self::Sale),
            "purchase_receipt" => Ok(Self::PurchaseReceipt),
            "repair_part_consumption" => Ok(Self::RepairPartConsumption),
            "manual_adjustment" => Ok(Self::ManualAdjustment),
            "invoice_void_reversal" => Ok(Self::InvoiceVoidReversal),
            "return_restock" => Ok(Self::ReturnRestock),
            "return_write_off" => Ok(Self::ReturnWriteOff),
            "return_supplier_rma" => Ok(Self::ReturnSupplierRma),
            "opening_balance" => Ok(Self::OpeningBalance),
            _ => Err(AppError::validation(format!(
                "Invalid stock movement type: {s}"
            ))),
        }
    }
}

/// A single recorded stock change (the audit trail entry behind a
/// `StockAdjustmentResponse`). `reference_id` links back to whatever
/// caused the movement (an order, a repair ticket) once other modules
/// start writing these; it's unused (`None`) for manual adjustments.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockMovement {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key of the movement record (e.g. stm_...).
    pub key: String,
    /// Product ID referencing the affected inventory item.
    pub product_id: String,
    /// The stock batch whose units moved. Absent only on the audit-only
    /// return movements (`ReturnWriteOff`/`ReturnSupplierRma`), which move no
    /// quantity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_key: Option<String>,
    /// Number of items added or deducted (+/-).
    pub quantity_delta: i64,
    /// Classification type for the stock movement.
    #[serde(rename = "type")]
    pub movement_type: StockMovementType,
    /// External reference key (e.g. invoice key, purchase key).
    pub reference_id: Option<String>,
    /// Optional explanatory note.
    pub note: Option<String>,
    /// Timestamp when movement occurred.
    pub created_at: DateTime<Utc>,
    /// Timestamp when movement was recorded/updated.
    pub updated_at: DateTime<Utc>,
    /// Version for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that recorded this movement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Query params for `GET /inventory/stock-movements`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct StockMovementListQuery {
    /// Filter movements by product ID.
    pub product_id: Option<String>,
    /// Filter movements by movement type.
    pub movement_type: Option<StockMovementType>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}

/// Response for `GET /inventory/stock-movements`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockMovementListResponse {
    /// List of stock movements for current page.
    pub items: Vec<StockMovement>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}

/// Response for `GET /products/{id}/movements`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StockMovementsResponse {
    /// List of historical stock movements for a specific product.
    pub movements: Vec<StockMovement>,
}

/// A subcategory as returned to API clients, persisted in its own
/// `subcategories` collection (see `modules::inventory::model::SubcategoryDocument`).
/// `category_key` is the FK back to its parent `CategoryInfo.key` — a
/// subcategory always belongs to exactly one category.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoryInfo {
    /// Unique business key identifying the subcategory (e.g. sub_...).
    pub key: String,
    /// Foreign key referencing the parent category key.
    pub category_key: String,
    /// Subcategory display name.
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Version counter.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this subcategory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// A main category and its subcategories, as returned to API clients.
/// `subcategories` is the authoritative allow-list — a product's
/// `subcategory_key` field is validated against it on create/update (see
/// `service::product::ensure_valid_category`).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    /// Unique business key identifying the category (e.g. cat_...).
    pub key: String,
    /// Category display name.
    pub name: String,
    /// Icon name/identifier.
    pub icon: String,
    /// Category color hex code or CSS color.
    pub color: String,
    /// Nested list of subcategories belonging to this category.
    pub subcategories: Vec<SubcategoryInfo>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Version counter.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Response for `GET /categories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoriesResponse {
    /// List of all categories with their subcategories.
    pub categories: Vec<CategoryInfo>,
}

/// Response for `GET /categories/{categoryKey}/subcategories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoriesResponse {
    /// Parent category key.
    pub category_key: String,
    /// Subcategories under the parent category.
    pub subcategories: Vec<SubcategoryInfo>,
}

/// One category option within `ValidCategoriesResponse` — key + display
/// name plus its own nested subcategory options, so a client (e.g. a
/// product-creation form) can render names while submitting keys.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoryOption {
    /// Category key.
    pub key: String,
    /// Category name.
    pub name: String,
    /// Subcategory selection options.
    pub subcategories: Vec<ValidSubcategoryOption>,
}

/// One subcategory option nested under `ValidCategoryOption`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidSubcategoryOption {
    /// Subcategory key.
    pub key: String,
    /// Subcategory name.
    pub name: String,
}

/// Response for `GET /categories/valid` — the same category/subcategory
/// data as `CategoriesResponse`, reshaped as key+name pairs for callers
/// (like a product form) that need to submit `categoryKey`/`subcategoryKey`
/// while displaying the human-readable name.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoriesResponse {
    /// List of category and subcategory selection options.
    pub categories: Vec<ValidCategoryOption>,
}

/// Body for `POST /categories` (admin-only — see `core::middleware::auth::AdminUser`).
/// `subcategories` is a bulk list of subcategory *names* — each becomes its
/// own `SubcategoryDocument` (with its own generated key) referencing the
/// newly created category, so a category can be bootstrapped with its
/// starter subcategories in one call.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    /// Display name of the new category.
    pub name: String,
    /// Icon name or identifier.
    pub icon: String,
    /// UI theme color code.
    pub color: String,
    /// Initial list of subcategory names to create under this category.
    #[serde(default)]
    pub subcategories: Vec<String>,
}

/// Body for `PUT /categories/{categoryKey}` (admin-only). All fields
/// optional, same partial-update convention as `UpdateProductRequest`.
/// Renaming (`name` set to a new value) is a pure display-label change —
/// products reference a category by its immutable `key`, so no cascade is
/// needed (see `service::category::update_category`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    /// Updated display name.
    pub name: Option<String>,
    /// Updated icon name.
    pub icon: Option<String>,
    /// Updated theme color code.
    pub color: Option<String>,
}

/// Body for `POST /categories/{categoryKey}/subcategories` (admin-only).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AddSubcategoryRequest {
    /// Name of the new subcategory to add.
    pub name: String,
}

/// Lifecycle of one physical serialized unit of a product. A unit starts
/// `InStock` when received on a purchase, becomes `Sold` at checkout, and
/// on a later credit-note return moves to exactly one of `ReturnedResalable`
/// / `ReturnedFaulty` / `UnderWarrantyClaim` / `WrittenOff` depending on the
/// return line's `condition`/`disposition` (see
/// `billing::service::credit_notes::create_credit_note`) — this is
/// per-unit history, kept separate from the product's aggregate
/// `stock_quantity` so an individual unit can be traced end to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SerialStatus {
    InStock,
    Sold,
    ReturnedResalable,
    ReturnedFaulty,
    UnderWarrantyClaim,
    WrittenOff,
}

impl SerialStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InStock => "in_stock",
            Self::Sold => "sold",
            Self::ReturnedResalable => "returned_resalable",
            Self::ReturnedFaulty => "returned_faulty",
            Self::UnderWarrantyClaim => "under_warranty_claim",
            Self::WrittenOff => "written_off",
        }
    }
}

impl std::str::FromStr for SerialStatus {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "in_stock" => Ok(Self::InStock),
            "sold" => Ok(Self::Sold),
            "returned_resalable" => Ok(Self::ReturnedResalable),
            "returned_faulty" => Ok(Self::ReturnedFaulty),
            "under_warranty_claim" => Ok(Self::UnderWarrantyClaim),
            "written_off" => Ok(Self::WrittenOff),
            _ => Err(AppError::validation(format!("Invalid serial status: {s}"))),
        }
    }
}

/// One physical serialized unit of a serialized product.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSerial {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key of this serial record (e.g. psn_...).
    pub key: String,
    /// Key of the product this unit belongs to.
    pub product_key: String,
    /// The manufacturer/shop serial number, unique across all products.
    pub serial_number: String,
    /// The stock batch this unit was received in — it sells at that batch's
    /// price and its cost is that batch's cost.
    pub batch_key: String,
    /// Current lifecycle status of this unit.
    pub status: SerialStatus,
    /// Key of the invoice this unit was sold on, once `Sold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_key: Option<String>,
    /// When this unit was sold, once `Sold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sold_at: Option<DateTime<Utc>>,
    /// Warranty length snapshotted from the product at the moment of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_months: Option<i64>,
    /// Computed `sold_at + warranty_months`, used to decide `within_warranty`
    /// on a later credit-note return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_expires_at: Option<DateTime<Utc>>,
    /// Key of the credit note that most recently returned this unit, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_note_key: Option<String>,
    /// Timestamp when this serial record was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when this serial record was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version.
    #[serde(default = "default_version")]
    pub version: i64,
}

/// What scanning a serial number resolves to: the unit, its product and the
/// price it sells at (its own batch's selling price). Admin-only cost fields are
/// stripped from `product` by the route, like every other product read.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SerialLookup {
    pub serial: ProductSerial,
    pub product: Product,
    pub selling_price_cents: i64,
}

/// Query params for `GET /products/{key}/serials`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ProductSerialListQuery {
    /// Filter by lifecycle status (e.g. `in_stock` for a sale-time picker).
    pub status: Option<SerialStatus>,
}

/// Response for `GET /products/{key}/serials`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSerialListResponse {
    /// Matching serial units for the requested product.
    pub items: Vec<ProductSerial>,
}
