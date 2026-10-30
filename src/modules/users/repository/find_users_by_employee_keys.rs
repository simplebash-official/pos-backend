use super::user_from_sqlite_row::user_from_sqlite_row;
use super::users::users;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::users::model::UserDocument;
use futures_util::TryStreamExt;

use mongodb::bson::{Bson, doc};

/// Batch variant of `find_user_by_employee_key` — used by
/// `service::find_user_summaries_by_employee_keys` to enrich a whole page of
/// employees with their login summary in one query instead of one lookup
/// per row.
pub(crate) async fn find_users_by_employee_keys(
    db: &Db,
    employee_keys: &[String],
) -> AppResult<Vec<UserDocument>> {
    if employee_keys.is_empty() {
        return Ok(Vec::new());
    }

    match db {
        Db::Mongo(db) => {
            let mut cursor = users(db)
                .find(doc! { "employee_key": { "$in": employee_keys }, "deleted_at": Bson::Null })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders: Vec<String> =
                (1..=employee_keys.len()).map(|i| format!("${i}")).collect();
            let query_str = format!(
                "SELECT * FROM users WHERE employee_key IN ({})",
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&query_str);
            for k in employee_keys {
                query = query.bind(k);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows.iter().map(user_from_sqlite_row).collect())
        }
    }
}
