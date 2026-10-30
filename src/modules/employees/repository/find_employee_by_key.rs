use super::employee_sqlite_row::EmployeeSqliteRow;
use super::employees::employees;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::employees::model::EmployeeDocument;
use mongodb::bson::doc;

pub(crate) async fn find_employee_by_key(
    db: &Db,
    key: &str,
) -> AppResult<Option<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(employees(db)
            .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let row = sqlx::query_as::<_, EmployeeSqliteRow>(
                "SELECT * FROM employees WHERE key = ? AND deleted_at IS NULL",
            )
            .bind(key)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(EmployeeSqliteRow::into_document))
        }
    }
}
