use super::row_to_batch_doc::row_to_batch_doc;
use super::stock_batches::stock_batches;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::inventory::model::StockBatchDocument;
use futures_util::TryStreamExt;

use mongodb::bson::doc;

/// The live batches of the given products that still hold units, oldest
/// first — the order a sale from a price tier consumes them in, and the input
/// to every product's `price_tiers`.
pub(crate) async fn list_in_stock_batches_for_products(
    db: &Db,
    product_keys: &[String],
) -> AppResult<Vec<StockBatchDocument>> {
    if product_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let cursor = stock_batches(db)
                .find(doc! {
                    "product_key": { "$in": product_keys },
                    "remaining_quantity": { "$gt": 0 },
                    "deleted_at": { "$exists": false },
                })
                .sort(doc! { "received_at": 1, "created_at": 1, "key": 1 })
                .await?;
            Ok(cursor.try_collect().await?)
        }
        Db::Sqlite(pool) => {
            let placeholders = vec!["?"; product_keys.len()].join(",");
            let sql = format!(
                "SELECT * FROM stock_batches WHERE product_key IN ({placeholders}) \
                 AND remaining_quantity > 0 AND deleted_at IS NULL \
                 ORDER BY received_at ASC, created_at ASC, key ASC"
            );
            let mut query = sqlx::query(&sql);
            for key in product_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(row_to_batch_doc).collect()
        }
    }
}
