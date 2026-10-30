use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::StockBatch;
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::repository;

/// The batch a no-receipt return at `selling_price_cents` goes back into:
/// the newest batch of the product at that price (empty ones included, so a
/// unit returned after its tier sold out still rejoins its own delivery).
/// `None` when the product has never been sold at that price.
pub(crate) async fn newest_batch_at_price(
    db: &Db,
    product_key: &str,
    selling_price_cents: i64,
) -> AppResult<Option<StockBatch>> {
    Ok(
        repository::stock_batch::list_batches_for_product(db, product_key)
            .await?
            .into_iter()
            .find(|batch| batch.selling_price_cents == selling_price_cents)
            .map(StockBatchDocument::into_stock_batch),
    )
}
