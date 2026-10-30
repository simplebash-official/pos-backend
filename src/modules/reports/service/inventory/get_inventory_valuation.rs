use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::InventoryValuationResponse;
use crate::modules::reports::repository;

/// Computes inventory stock valuation at cost vs retail price, potential profit, and low-stock alerts.
pub(crate) async fn get_inventory_valuation(db: &Db) -> AppResult<InventoryValuationResponse> {
    repository::inventory_valuation::aggregate_inventory_valuation(db).await
}
