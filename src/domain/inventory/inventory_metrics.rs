use serde::Serialize;
use utoipa::ToSchema;

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
