use super::row_to_batch_doc::row_to_batch_doc;
use super::stock_batches::stock_batches;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::StockBatchDocument;
use futures_util::TryStreamExt;

use mongodb::bson::doc;

/// Every live batch of one product, newest first — the intake history shown
/// on the product drawer, empty batches included.
pub(crate) async fn list_batches_for_product(
    db: &Db,
    product_key: &str,
) -> AppResult<Vec<StockBatchDocument>> {
    match db {
        Db::Mongo(db) => {
            let cursor = stock_batches(db)
                .find(doc! { "product_key": product_key, "deleted_at": { "$exists": false } })
                .sort(doc! { "received_at": -1, "created_at": -1, "key": -1 })
                .await?;
            Ok(cursor.try_collect().await?)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM stock_batches WHERE product_key = ? AND deleted_at IS NULL \
                 ORDER BY received_at DESC, created_at DESC, key DESC",
            )
            .bind(product_key)
            .fetch_all(pool)
            .await?;
            rows.iter().map(row_to_batch_doc).collect()
        }
    }
}
