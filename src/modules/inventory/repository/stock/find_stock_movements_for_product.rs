use super::row_to_stock_movement_doc::row_to_stock_movement_doc;
use super::stock_movements::stock_movements;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::model::StockMovementDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

/// Sorted oldest-first so `GET /products/{id}/movements` reads as a
/// chronological history rather than most-recent-first.
pub(crate) async fn find_stock_movements_for_product(
    db: &Db,
    product_id: ObjectId,
) -> AppResult<Vec<StockMovementDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = stock_movements(db)
                .find(doc! { "product_id": product_id, "deleted_at": { "$exists": false } })
                .sort(doc! { "created_at": 1 })
                .await?;

            let mut movements = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                movements.push(document);
            }
            Ok(movements)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM stock_movements WHERE product_id = ? AND deleted_at IS NULL ORDER BY created_at ASC",
            )
            .bind(product_id.to_hex())
            .fetch_all(pool)
            .await?;

            rows.iter().map(row_to_stock_movement_doc).collect()
        }
    }
}
