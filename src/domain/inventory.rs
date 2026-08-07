// Pure business types for the inventory feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::inventory::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A product as returned to API clients. `id` (the Mongo `ObjectId` as a
/// hex string) is the stable route/lookup key; `key` is the human-shareable
/// prefixed id (see `core::id::generate_id`) — both are exposed since
/// different callers reference a product differently. `category`/
/// `category_key` (and their subcategory counterparts) are both present:
/// the `*_key` fields are the actual foreign keys stored on the product
/// (see `modules::inventory::model::ProductDocument`), while the bare
/// `category`/`subcategory` names are resolved at read time purely for
/// display so callers don't have to cross-reference `GET /categories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: String,
    pub key: String,
    pub sku: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub name: String,
    pub category_key: String,
    pub category: String,
    pub subcategory_key: String,
    pub subcategory: String,
    pub cost_price_cents: i64,
    pub selling_price_cents: i64,
    pub stock_quantity: i64,
    pub min_stock_threshold: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body for `POST /products`. `sku` is deliberately absent — it's
/// generated server-side from the category/subcategory names resolved from
/// `category_key`/`subcategory_key` (see `service::sku::generate_sku`), not
/// client-supplied. `category_key`/`subcategory_key` must be the system-
/// generated `key` of an existing category/subcategory (see `GET
/// /categories` or `GET /categories/valid`) — never the display name.
/// Prices are integer cents (never float) to avoid rounding drift;
/// `stock_quantity`/`min_stock_threshold` default to `0` via
/// `#[serde(default)]` so a minimal request still deserializes.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProductRequest {
    #[serde(default)]
    pub barcode: Option<String>,
    pub name: String,
    pub category_key: String,
    pub subcategory_key: String,
    pub cost_price_cents: i64,
    pub selling_price_cents: i64,
    #[serde(default)]
    pub stock_quantity: i64,
    #[serde(default)]
    pub min_stock_threshold: i64,
}

/// Body for `PUT /products/{id}`. Every field is optional so a client can
/// send only what changed — `service::product::update_product` fills in
/// omitted fields from the existing document rather than clearing them.
/// `category_key`/`subcategory_key`, like on create, must be keys, not names.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProductRequest {
    pub barcode: Option<String>,
    pub name: Option<String>,
    pub category_key: Option<String>,
    pub subcategory_key: Option<String>,
    pub cost_price_cents: Option<i64>,
    pub selling_price_cents: Option<i64>,
    pub stock_quantity: Option<i64>,
    pub min_stock_threshold: Option<i64>,
}

/// Query params for `GET /products`. `sort_by` is whitelisted against a
/// fixed set of fields before it ever reaches a Mongo `sort` document (see
/// `service::product::sort_field_for`) rather than passed through directly.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ProductListQuery {
    pub search: Option<String>,
    pub category_key: Option<String>,
    pub subcategory_key: Option<String>,
    pub low_stock: Option<bool>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// Query params for `GET /overview`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct InventoryOverviewQuery {
    pub search: Option<String>,
    pub category_key: Option<String>,
    pub subcategory_key: Option<String>,
    pub low_stock: Option<bool>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
}

/// Top-level inventory metrics for the dashboard header.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryMetrics {
    pub total_items: u64,
    pub total_categories: u64,
    pub total_subcategories: u64,
    pub low_stock_alerts: u64,
}

/// Subcategory accordion section containing its matching products list and pagination meta.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventorySubcategoryOverview {
    pub key: String,
    pub category_key: String,
    pub name: String,
    pub total_items: u64,
    pub products: Vec<Product>,
    pub pagination: PaginationMeta,
}

/// Category accordion section containing nested subcategories and aggregate counts.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryCategoryOverview {
    pub key: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub total_items: u64,
    pub subcategories_count: u64,
    pub subcategories: Vec<InventorySubcategoryOverview>,
}

/// Response payload for `GET /api/inventory/overview`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOverviewResponse {
    pub metrics: InventoryMetrics,
    pub categories: Vec<InventoryCategoryOverview>,
}

/// Pagination metadata attached to any paginated list response.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    pub page: u64,
    pub limit: u64,
    pub total: u64,
    pub total_pages: u64,
}

/// Response body for `GET /products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductListResponse {
    pub items: Vec<Product>,
    pub pagination: PaginationMeta,
}

/// Body for `DELETE /products` (batch delete). Any id that fails to parse
/// as an `ObjectId` is silently dropped rather than rejecting the whole
/// request — see `service::product::delete_products`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsRequest {
    pub product_ids: Vec<String>,
}

/// Response for the batch-delete endpoint — count only, since the caller
/// already knows which ids it asked to delete.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsResponse {
    pub deleted_count: u64,
}

/// Body for `PATCH /products/{id}/stock`. `delta` is signed (positive =
/// receipt, negative = consumption/sale) rather than an absolute target
/// quantity, so concurrent adjustments compose instead of racing to
/// overwrite each other's target value.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct StockAdjustmentRequest {
    pub delta: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Response for a stock adjustment — includes both the before/after
/// quantities so a client can display the change without a second request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockAdjustmentResponse {
    pub id: String,
    pub key: String,
    pub sku: String,
    pub name: String,
    pub stock_quantity: i64,
    pub previous_stock_quantity: i64,
    pub delta: i64,
    pub updated_at: DateTime<Utc>,
}

/// One entry in the low-stock report. `deficit` (threshold minus current
/// quantity) is precomputed here so clients don't each reimplement the
/// subtraction.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LowStockItem {
    pub id: String,
    pub key: String,
    pub sku: String,
    pub name: String,
    pub stock_quantity: i64,
    pub min_stock_threshold: i64,
    pub deficit: i64,
}

/// Response for `GET /products/low-stock`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LowStockResponse {
    pub items: Vec<LowStockItem>,
    pub total: u64,
}

/// Why a stock movement happened. Every adjustment (manual or automated)
/// gets recorded as a `StockMovement` tagged with one of these, so the
/// movement history stays auditable even once the originating request is
/// long gone. Only `ManualAdjustment` is produced by the API today (via
/// `PATCH /products/{id}/stock`); the others are reserved for when
/// billing/repairs start writing movements directly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementType {
    Sale,
    PurchaseReceipt,
    RepairPartConsumption,
    ManualAdjustment,
    Return,
}

/// A single recorded stock change (the audit trail entry behind a
/// `StockAdjustmentResponse`). `reference_id` links back to whatever
/// caused the movement (an order, a repair ticket) once other modules
/// start writing these; it's unused (`None`) for manual adjustments.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockMovement {
    pub id: String,
    pub key: String,
    pub product_id: String,
    pub quantity_delta: i64,
    #[serde(rename = "type")]
    pub movement_type: StockMovementType,
    pub reference_id: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response for `GET /products/{id}/movements`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StockMovementsResponse {
    pub movements: Vec<StockMovement>,
}

/// A subcategory as returned to API clients, persisted in its own
/// `subcategories` collection (see `modules::inventory::model::SubcategoryDocument`).
/// `category_key` is the FK back to its parent `CategoryInfo.key` — a
/// subcategory always belongs to exactly one category.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoryInfo {
    pub key: String,
    pub category_key: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A main category and its subcategories, as returned to API clients.
/// `subcategories` is the authoritative allow-list — a product's
/// `subcategory_key` field is validated against it on create/update (see
/// `service::product::ensure_valid_category`).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    pub key: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub subcategories: Vec<SubcategoryInfo>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response for `GET /categories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoriesResponse {
    pub categories: Vec<CategoryInfo>,
}

/// Response for `GET /categories/{categoryKey}/subcategories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoriesResponse {
    pub category_key: String,
    pub subcategories: Vec<SubcategoryInfo>,
}

/// One category option within `ValidCategoriesResponse` — key + display
/// name plus its own nested subcategory options, so a client (e.g. a
/// product-creation form) can render names while submitting keys.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoryOption {
    pub key: String,
    pub name: String,
    pub subcategories: Vec<ValidSubcategoryOption>,
}

/// One subcategory option nested under `ValidCategoryOption`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidSubcategoryOption {
    pub key: String,
    pub name: String,
}

/// Response for `GET /categories/valid` — the same category/subcategory
/// data as `CategoriesResponse`, reshaped as key+name pairs for callers
/// (like a product form) that need to submit `categoryKey`/`subcategoryKey`
/// while displaying the human-readable name.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoriesResponse {
    pub categories: Vec<ValidCategoryOption>,
}

/// Body for `POST /categories` (admin-only — see `core::middleware::auth::AdminUser`).
/// `subcategories` is a bulk list of subcategory *names* — each becomes its
/// own `SubcategoryDocument` (with its own generated key) referencing the
/// newly created category, so a category can be bootstrapped with its
/// starter subcategories in one call.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub icon: String,
    pub color: String,
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
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// Body for `POST /categories/{categoryKey}/subcategories` (admin-only).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AddSubcategoryRequest {
    pub name: String,
}
