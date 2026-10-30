use super::from_row::from_row;
use super::templates::templates;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::recurring_expenses::model::RecurringExpenseDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every regular bill, stopped ones included; the service filters.
pub(crate) async fn list_all(db: &Db) -> AppResult<Vec<RecurringExpenseDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut cursor = templates(db).find(doc! {}).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let rows = sqlx::query("SELECT * FROM recurring_expenses")
                .fetch_all(pool)
                .await?;
            rows.iter().map(from_row).collect()
        }
    }
}
