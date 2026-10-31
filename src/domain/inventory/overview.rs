//! The inventory overview by category and subcategory.

use super::product::PaginationMeta;
use super::product::Product;

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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

/// Response payload for `GET /api/inventory/overview`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOverviewResponse {
    /// Summary inventory metrics.
    pub metrics: InventoryMetrics,
    /// Grouped category hierarchical data.
    pub categories: Vec<InventoryCategoryOverview>,
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
