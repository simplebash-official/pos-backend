use super::employee_sqlite_row::EmployeeSqliteRow;
use super::employees::employees;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::employees::model::EmployeeDocument;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn find_employee_by_id(
    db: &Db,
    id: ObjectId,
) -> AppResult<Option<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(employees(db)
            .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let row = sqlx::query_as::<_, EmployeeSqliteRow>(
                "SELECT * FROM employees WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&id_str)
            .fetch_optional(pool)
            .await?;
            Ok(row.map(EmployeeSqliteRow::into_document))
        }
    }
}
