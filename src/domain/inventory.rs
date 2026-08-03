// Pure business types for the inventory feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::inventory::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProductRequest {
    pub sku: String,
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

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    pub page: u64,
    pub limit: u64,
    pub total: u64,
    pub total_pages: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductListResponse {
    pub items: Vec<Product>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsRequest {
    pub product_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsResponse {
    pub deleted_count: u64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct StockAdjustmentRequest {
    pub delta: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LowStockResponse {
    pub items: Vec<LowStockItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementType {
    Sale,
    PurchaseReceipt,
    RepairPartConsumption,
    ManualAdjustment,
    Return,
}

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
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StockMovementsResponse {
    pub movements: Vec<StockMovement>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    pub key: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub subcategories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoriesResponse {
    pub categories: Vec<CategoryInfo>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoriesResponse {
    pub category: String,
    pub subcategories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoriesResponse {
    pub valid_categories: Vec<String>,
    pub category_subcategory_map: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub icon: String,
    pub color: String,
    #[serde(default)]
    pub subcategories: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AddSubcategoryRequest {
    pub subcategory: String,
}
