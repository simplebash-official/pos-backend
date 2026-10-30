use super::plan_from_batches::plan_from_batches;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::BatchAllocation;

use crate::modules::inventory::repository;

/// `plan_from_batches` against the product's live in-stock batches.
pub(crate) async fn plan_tier_allocation(
    db: &Db,
    product_key: &str,
    product_name: &str,
    selling_price_cents: i64,
    quantity: i64,
) -> AppResult<Vec<BatchAllocation>> {
    let batches =
        repository::stock_batch::list_in_stock_batches_for_products(db, &[product_key.to_string()])
            .await?;
    plan_from_batches(&batches, product_name, selling_price_cents, quantity)
}
