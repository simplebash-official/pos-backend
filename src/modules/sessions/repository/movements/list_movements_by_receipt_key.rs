use super::cash_movements_col::cash_movements_col;
use super::movement_from_sqlite_row::movement_from_sqlite_row;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::model::CashMovementDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// The movements recorded with one supplier receipt (the payment, and the owner's capital put in
/// for it), reversals left out.
pub async fn list_movements_by_receipt_key(
    db: &Db,
    receipt_key: &str,
) -> AppResult<Vec<CashMovementDocument>> {
    match db {
        Db::Mongo(mongo_db) => {
            let mut cursor = cash_movements_col(mongo_db)
                .find(doc! { "receipt_key": receipt_key })
                .await?;
            let mut out = Vec::new();
            while let Some(item) = cursor.try_next().await? {
                out.push(item);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM cash_movements WHERE receipt_key = ?")
                .bind(receipt_key)
                .fetch_all(pool)
                .await?;
            rows.iter().map(movement_from_sqlite_row).collect()
        }
    }
}
