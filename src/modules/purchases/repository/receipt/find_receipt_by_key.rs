use super::receipt_from_row::receipt_from_row;
use super::receipts::receipts;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseReceiptDocument;

use mongodb::bson::doc;

/// Fetches one receipt by its key.
pub(crate) async fn find_receipt_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<PurchaseReceiptDocument>> {
    match db {
        Db::Mongo(db) => Ok(receipts(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM purchase_receipts WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(receipt_from_row))
        }
    }
}
