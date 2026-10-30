use super::expense_from_row::expense_from_row;
use super::expenses::expenses;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpenseDocument;
use mongodb::bson::doc;

/// One expense by key.
pub(crate) async fn find_expense_by_key(db: &Db, key: &str) -> AppResult<Option<ExpenseDocument>> {
    match db {
        Db::Mongo(db) => Ok(expenses(db).find_one(doc! { "key": key }).await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query("SELECT * FROM expenses WHERE key = ?")
                .bind(key)
                .fetch_optional(pool)
                .await?;
            Ok(row.as_ref().map(expense_from_row))
        }
    }
}
