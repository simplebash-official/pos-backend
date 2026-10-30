use super::pagination_meta::PaginationMeta;
use super::product::Product;

use serde::Serialize;
use utoipa::ToSchema;

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
