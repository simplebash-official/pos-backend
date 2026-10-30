use super::in_stock_units::InStockUnits;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::repository;

/// The in-stock units of the given products, one entry per batch. Uses the
/// batch's `created_at` (when it was entered), not its `received_at`, which a
/// person may backdate.
pub(crate) async fn in_stock_units(
    db: &Db,
    product_keys: &[String],
) -> AppResult<Vec<InStockUnits>> {
    Ok(
        repository::stock_batch::list_in_stock_batches_for_products(db, product_keys)
            .await?
            .into_iter()
            .map(|batch| InStockUnits {
                product_key: batch.product_key,
                remaining_quantity: batch.remaining_quantity,
                created_at: batch.created_at.to_chrono(),
            })
            .collect(),
    )
}
