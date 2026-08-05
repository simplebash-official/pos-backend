// Pure business types for the inventory feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::inventory::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A product as returned to API clients. `id` (the Mongo `ObjectId` as a
/// hex string) is the stable route/lookup key; `key` is the human-shareable
/// prefixed id (see `core::id::generate_id`) — both are exposed since
/// different callers reference a product differently.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: String,
    pub key: String,
    pub sku: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub name: String,
    pub category: String,
    pub subcategory: String,
    pub cost_price_cents: i64,
    pub selling_price_cents: i64,
    pub stock_quantity: i64,
    pub min_stock_threshold: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body for `POST /products`. `sku` is deliberately absent — it's
/// generated server-side from `category`/`subcategory` (see
/// `service::sku::generate_sku`), not client-supplied. Prices are integer
/// cents (never float) to avoid rounding drift; `stock_quantity`/
/// `min_stock_threshold` default to `0` via `#[serde(default)]` so a
/// minimal request still deserializes.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProductRequest {
    #[serde(default)]
    pub barcode: Option<String>,
    pub name: String,
    pub category: String,
    pub subcategory: String,
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
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProductRequest {
    pub barcode: Option<String>,
    pub name: Option<String>,
    pub category: Option<String>,
    pub subcategory: Option<String>,
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
    pub category: Option<String>,
    pub subcategory: Option<String>,
    pub low_stock: Option<bool>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
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

/// A main category and its allowed subcategories, as returned to API
/// clients. `subcategories` is the authoritative allow-list — a product's
/// `subcategory` field is validated against it on create/update (see
/// `service::product::ensure_valid_category`).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    pub key: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub subcategories: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Response for `GET /categories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoriesResponse {
    pub categories: Vec<CategoryInfo>,
}

/// Response for `GET /categories/{category}/subcategories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoriesResponse {
    pub category: String,
    pub subcategories: Vec<String>,
}

/// Response for `GET /categories/valid` — the same category/subcategory
/// data as `CategoriesResponse`, reshaped into a flat name list plus a
/// name-to-subcategories map for callers (like a product form) that want
/// direct lookup instead of scanning a `Vec<CategoryInfo>`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoriesResponse {
    pub valid_categories: Vec<String>,
    pub category_subcategory_map: HashMap<String, Vec<String>>,
}

/// Body for `POST /categories` (admin-only — see `core::middleware::auth::AdminUser`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub icon: String,
    pub color: String,
    #[serde(default)]
    pub subcategories: Vec<String>,
}

/// Body for `PUT /categories/{category}` (admin-only). All fields optional,
/// same partial-update convention as `UpdateProductRequest`. Renaming
/// (`name` set to a new value) cascades to every product referencing the
/// old category name — see `service::category::update_category`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// Body for `POST /categories/{category}/subcategories` (admin-only).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AddSubcategoryRequest {
    pub subcategory: String,
}
