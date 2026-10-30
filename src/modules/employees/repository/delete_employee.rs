use super::employees::employees;
use super::find_employee_by_id::find_employee_by_id;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::modules::employees::model::EmployeeDocument;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn delete_employee(
    db: &Db,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => {
            let now = BsonDateTime::now();
            let mut set_doc = doc! {
                "deleted_at": now,
                "updated_at": now,
            };
            if let Some(device) = device_id {
                set_doc.insert("updated_by_device", device);
            }
            Ok(employees(db)
                .find_one_and_update(
                    doc! { "_id": id, "deleted_at": { "$exists": false } },
                    doc! {
                        "$set": set_doc,
                        "$inc": { "version": 1 }
                    },
                )
                .return_document(ReturnDocument::After)
                .await?)
        }
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_employee_by_id(db, id).await?;
            let Some(mut emp) = existing else {
                return Ok(None);
            };

            let now = BsonDateTime::now();
            emp.deleted_at = Some(now);
            emp.updated_at = now;
            emp.version += 1;
            emp.updated_by_device = device_id.clone();

            let now_iso = bson_to_iso(&now);

            sqlx::query(
                "UPDATE employees SET deleted_at = ?, updated_at = ?, updated_by_device = ?, version = version + 1 WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(&now_iso)
            .bind(&now_iso)
            .bind(&device_id)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(emp))
        }
    }
}
