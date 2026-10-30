use super::from_row::from_row;
use super::templates::templates;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use mongodb::bson::doc;

/// One regular bill by key.
pub(crate) async fn find_by_key(db: &Db, key: &str) -> AppResult<Option<RecurringExpenseDocument>> {
    match db {
        Db::Mongo(db) => Ok(templates(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM recurring_expenses WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            row.as_ref().map(from_row).transpose()
        }
    }
}
