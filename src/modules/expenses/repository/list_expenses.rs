use super::expense_from_row::expense_from_row;
use super::expenses::expenses;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::expenses::model::ExpenseDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

/// Every expense dated between two days (`YYYY-MM-DD`, inclusive; `None` is open-ended).
pub(crate) async fn list_expenses(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<Vec<ExpenseDocument>> {
    match db {
        Db::Mongo(db) => {
            let mut range = doc! {};
            if let Some(from) = from {
                range.insert("$gte", from);
            }
            if let Some(to) = to {
                range.insert("$lte", to);
            }
            let filter = if range.is_empty() {
                doc! {}
            } else {
                doc! { "expense_date": range }
            };
            let mut cursor = expenses(db).find(filter).await?;
            let mut out = Vec::new();
            while let Some(d) = cursor.try_next().await? {
                out.push(d);
            }
            Ok(out)
        }
        Db::Sqlite(pool) => {
            let mut sql = String::from("SELECT * FROM expenses WHERE 1 = 1");
            if from.is_some() {
                sql.push_str(" AND expense_date >= ?");
            }
            if to.is_some() {
                sql.push_str(" AND expense_date <= ?");
            }
            let mut query = sqlx::query(&sql);
            if let Some(from) = from {
                query = query.bind(from);
            }
            if let Some(to) = to {
                query = query.bind(to);
            }
            Ok(query
                .fetch_all(pool)
                .await?
                .iter()
                .map(expense_from_row)
                .collect())
        }
    }
}
