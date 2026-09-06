use super::dates;
use crate::{
    clients::db::Db,
    core::error::AppResult,
    domain::reports::{InventoryValuationResponse, TopProductsQuery, TopProductsResponse},
    modules::reports::repository,
};

/// Computes inventory stock valuation at cost vs retail price, potential profit, and low-stock alerts.
pub(crate) async fn get_inventory_valuation(db: &Db) -> AppResult<InventoryValuationResponse> {
    repository::inventory_valuation::aggregate_inventory_valuation(db).await
}

/// Generates a list of top-selling products ranked by revenue or volume sold.
pub(crate) async fn get_top_products(
    db: &Db,
    query: TopProductsQuery,
) -> AppResult<TopProductsResponse> {
    let (start, end) = dates::parse_date_range(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;

    let limit = query.limit.unwrap_or(10).clamp(1, 100);
    let sort_by = query.sort_by.as_deref().unwrap_or("revenue");

    let products =
        repository::sales::aggregate_top_products(db, start, end, limit, sort_by).await?;

    let mut total_units_sold = 0;
    let mut total_revenue_cents = 0;

    for p in &products {
        total_units_sold += p.units_sold;
        total_revenue_cents += p.total_revenue_cents;
    }

    Ok(TopProductsResponse {
        products,
        total_units_sold,
        total_revenue_cents,
    })
}
