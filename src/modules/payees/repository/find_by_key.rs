use super::from_row::from_row;
use super::payees::payees;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::payees::model::PayeeDocument;
use mongodb::bson::doc;

/// One stored payee by key.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<PayeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(payees(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM payees WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}
