use super::employees::employees;
use super::find_employee_by_id::find_employee_by_id;
use crate::clients::db::Db;
use crate::clients::sqlite::bson_to_iso;
use crate::core::error::AppResult;
use crate::modules::employees::model::EmployeeDocument;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc};
use mongodb::options::ReturnDocument;

pub(crate) async fn update_employee(
    db: &Db,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<EmployeeDocument>> {
    match db {
        Db::Mongo(db) => Ok(employees(db)
            .find_one_and_update(
                doc! { "_id": id, "deleted_at": { "$exists": false } },
                doc! { "$set": set_doc, "$inc": { "version": 1 } },
            )
            .return_document(ReturnDocument::After)
            .await?),
        Db::Sqlite(pool) => {
            let id_str = id.to_hex();
            let existing = find_employee_by_id(db, id).await?;
            let Some(mut emp) = existing else {
                return Ok(None);
            };

            if let Ok(name) = set_doc.get_str("name") {
                emp.name = name.to_string();
            }
            if let Ok(phone) = set_doc.get_str("phone") {
                emp.phone = phone.to_string();
            }
            if set_doc.contains_key("nic_or_id") {
                emp.nic_or_id = set_doc.get_str("nic_or_id").ok().map(|s| s.to_string());
            }
            if let Ok(role) = set_doc.get_str("role")
                && let Ok(r) = role.parse()
            {
                emp.role = r;
            }
            if let Ok(split_type) = set_doc.get_str("default_split_type")
                && let Ok(st) = split_type.parse()
            {
                emp.default_split_type = st;
            }
            if let Ok(split_value) = set_doc.get_f64("default_split_value") {
                emp.default_split_value = split_value;
            }
            if let Ok(status) = set_doc.get_str("status")
                && let Ok(s) = status.parse()
            {
                emp.status = s;
            }
            if set_doc.contains_key("notes") {
                emp.notes = set_doc.get_str("notes").ok().map(|s| s.to_string());
            }
            if set_doc.contains_key("updated_by_device") {
                emp.updated_by_device = set_doc
                    .get_str("updated_by_device")
                    .ok()
                    .map(|s| s.to_string());
            }
            emp.version += 1;
            emp.updated_at = BsonDateTime::now();

            let updated_at_iso = bson_to_iso(&emp.updated_at);

            sqlx::query(
                r#"
                UPDATE employees SET
                    name = ?, phone = ?, nic_or_id = ?, role = ?, default_split_type = ?,
                    default_split_value = ?, status = ?, notes = ?, updated_by_device = ?,
                    version = ?, updated_at = ?
                WHERE id = ? AND deleted_at IS NULL
                "#,
            )
            .bind(&emp.name)
            .bind(&emp.phone)
            .bind(&emp.nic_or_id)
            .bind(emp.role.as_str())
            .bind(emp.default_split_type.as_str())
            .bind(emp.default_split_value)
            .bind(emp.status.as_str())
            .bind(&emp.notes)
            .bind(&emp.updated_by_device)
            .bind(emp.version)
            .bind(&updated_at_iso)
            .bind(&id_str)
            .execute(pool)
            .await?;

            Ok(Some(emp))
        }
    }
}
