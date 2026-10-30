use super::purchases::purchases;
use super::row_to_purchase_doc::row_to_purchase_doc;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every live line of the given receipts, oldest first, so a receipt's lines read in the order
/// they were entered.
pub(crate) async fn list_purchases_by_receipts(
    db: &Db,
    receipt_keys: &[String],
) -> AppResult<Vec<PurchaseDocument>> {
    if receipt_keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = purchases(db)
                .find(doc! { "receipt_key": { "$in": receipt_keys }, "deleted_at": { "$exists": false } })
                .sort(doc! { "created_at": 1, "key": 1 })
                .await?;
            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders = vec!["?"; receipt_keys.len()].join(", ");
            let sql = format!(
                "SELECT * FROM purchases WHERE receipt_key IN ({placeholders}) AND deleted_at IS NULL ORDER BY created_at ASC, key ASC"
            );
            let mut query = sqlx::query(&sql);
            for key in receipt_keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            rows.iter().map(row_to_purchase_doc).collect()
        }
    }
}
