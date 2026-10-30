use super::inventory_subcategory_overview::InventorySubcategoryOverview;

use serde::Serialize;
use utoipa::ToSchema;

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
