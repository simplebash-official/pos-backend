use super::batch_not_found::batch_not_found;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::StockBatch;
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::repository;

/// Loads one live batch, requiring it to belong to `product_key`.
pub(crate) async fn get_batch_for_product(
    db: &Db,
    batch_key: &str,
    product_key: &str,
) -> AppResult<StockBatch> {
    repository::stock_batch::find_batch_by_key(db, batch_key)
        .await?
        .filter(|batch| batch.product_key == product_key)
        .map(StockBatchDocument::into_stock_batch)
        .ok_or_else(|| batch_not_found(batch_key))
}
