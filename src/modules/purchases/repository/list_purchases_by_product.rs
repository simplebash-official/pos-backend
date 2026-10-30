use super::purchases::purchases;
use super::row_to_purchase_doc::row_to_purchase_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

#[allow(dead_code)]
pub(crate) async fn list_purchases_by_product(
    db: &Db,
    product_key: &str,
) -> AppResult<Vec<PurchaseDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = purchases(db)
                .find(doc! { "product_key": product_key })
                .sort(doc! { "date": -1 })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query(
                "SELECT * FROM purchases WHERE product_key = ? AND deleted_at IS NULL ORDER BY date DESC",
            )
            .bind(product_key)
            .fetch_all(pool)
            .await?;

            rows.iter().map(row_to_purchase_doc).collect()
        }
    }
}
