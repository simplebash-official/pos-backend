use super::employees::employees;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};
use crate::core::error::AppResult;
use crate::modules::employees::model::EmployeeDocument;
use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_employee(
    db: &Db,
    mut document: EmployeeDocument,
) -> AppResult<EmployeeDocument> {
    match db {
        Db::Mongo(db) => {
            let result = employees(db).insert_one(&document).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Db::Sqlite(pool) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let created_at = bson_to_iso(&document.created_at);
            let updated_at = bson_to_iso(&document.updated_at);
            let deleted_at = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO employees (
                    key, id, name, phone, nic_or_id, role, default_split_type,
                    default_split_value, status, notes, version, created_at,
                    updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(&document.phone)
            .bind(&document.nic_or_id)
            .bind(document.role.as_str())
            .bind(document.default_split_type.as_str())
            .bind(document.default_split_value)
            .bind(document.status.as_str())
            .bind(&document.notes)
            .bind(document.version)
            .bind(&created_at)
            .bind(&updated_at)
            .bind(&deleted_at)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
