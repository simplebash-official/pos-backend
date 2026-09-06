// Orchestration for the lightweight Inventory & Stock KPI-card endpoint
// (`GET /inventory/stats`) — deliberately separate from `overview`, which
// returns the whole category/subcategory/paginated-product tree; this one
// exists purely so the frontend's dashboard header doesn't have to fetch
// that entire tree just to render 4 numbers.

use crate::{
    clients::db::Db, core::error::AppResult, domain::inventory::InventoryMetrics,
    modules::inventory::repository,
};

/// Computes the 4 KPI cards for the Inventory & Stock screen's dashboard
/// header. Reuses `InventoryMetrics` — the same DTO `/inventory/overview`
/// embeds — since the numbers are conceptually identical.
pub async fn get_inventory_stats(db: &Db) -> AppResult<InventoryMetrics> {
    let (total_items, low_stock_alerts) = repository::product::count_stats(db).await?;
    let total_categories = repository::category::count_categories(db).await?;
    let total_subcategories = repository::subcategory::count_subcategories(db).await?;

    Ok(InventoryMetrics {
        total_items,
        total_categories,
        total_subcategories,
        low_stock_alerts,
    })
}
