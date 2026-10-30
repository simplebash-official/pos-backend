use super::inventory_category_overview::InventoryCategoryOverview;
use super::inventory_metrics::InventoryMetrics;

use serde::Serialize;
use utoipa::ToSchema;

/// Response payload for `GET /api/inventory/overview`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryOverviewResponse {
    /// Summary inventory metrics.
    pub metrics: InventoryMetrics,
    /// Grouped category hierarchical data.
    pub categories: Vec<InventoryCategoryOverview>,
}
