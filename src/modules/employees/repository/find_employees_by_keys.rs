use super::employee_sqlite_row::EmployeeSqliteRow;
use super::employees::employees;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::employees::model::EmployeeDocument;
use futures_util::TryStreamExt;
use mongodb::bson::doc;

pub(crate) async fn find_employees_by_keys(
    db: &Db,
    keys: &[String],
) -> AppResult<Vec<EmployeeDocument>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    match db {
        Db::Mongo(db) => {
            let mut cursor = employees(db)
                .find(doc! { "key": { "$in": keys }, "deleted_at": { "$exists": false } })
                .await?;

            let mut items = Vec::new();
            while let Some(document) = cursor.try_next().await? {
                items.push(document);
            }
            Ok(items)
        }
        Db::Sqlite(pool) => {
            let placeholders = keys.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT * FROM employees WHERE key IN ({placeholders}) AND deleted_at IS NULL"
            );
            let mut query = sqlx::query_as::<_, EmployeeSqliteRow>(&sql);
            for key in keys {
                query = query.bind(key);
            }
            let rows = query.fetch_all(pool).await?;
            Ok(rows
                .into_iter()
                .map(EmployeeSqliteRow::into_document)
                .collect())
        }
    }
}
