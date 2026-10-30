use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::StockBatchListResponse;
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::repository;

/// All batches of a product, newest first (`GET /products/{key}/batches`).
pub(crate) async fn list_batches(db: &Db, product_key: &str) -> AppResult<StockBatchListResponse> {
    super::super::product::get_product_by_key(db, product_key).await?;
    let items = repository::stock_batch::list_batches_for_product(db, product_key)
        .await?
        .into_iter()
        .map(StockBatchDocument::into_stock_batch)
        .collect();
    Ok(StockBatchListResponse { items })
}
