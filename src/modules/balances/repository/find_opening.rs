use super::opening_from_row::opening_from_row;
use super::openings::openings;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::balances::model::BalanceOpeningDocument;
use mongodb::bson::doc;

/// One opening by key.
pub(crate) async fn find_opening(db: &Db, key: &str) -> AppResult<Option<BalanceOpeningDocument>> {
    match db {
        Db::Mongo(db) => Ok(openings(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM balance_openings WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(opening_from_row).transpose()
        }
    }
}
