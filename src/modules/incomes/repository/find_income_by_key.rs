use super::income_from_row::income_from_row;
use super::incomes::incomes;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::incomes::model::IncomeDocument;
use mongodb::bson::doc;

/// One income by key.
pub(crate) async fn find_income_by_key(db: &Db, key: &str) -> AppResult<Option<IncomeDocument>> {
    match db {
        Db::Mongo(db) => Ok(incomes(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM incomes WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(income_from_row).transpose()
        }
    }
}
