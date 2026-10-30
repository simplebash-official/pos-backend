use super::repairs::repairs;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;
use crate::modules::repairs::model::RepairDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert(db: &Db, mut document: RepairDocument) -> AppResult<RepairDocument> {
    match db {
        Db::Mongo(db) => {
            let result = repairs(db).insert_one(&document).await?;
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
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let deleted_at_iso = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO repairs (
                    key, id, ticket_number, customer_key, customer_name, customer_phone,
                    device_model, serial_number, issue_description, promised_ready_at,
                    status, estimated_cost_cents, material_cost_cents, assigned_employee_id,
                    assigned_employee_name, split_type, split_value, version,
                    created_at, updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.ticket_number)
            .bind(&document.customer_key)
            .bind(&document.customer_name)
            .bind(&document.customer_phone)
            .bind(&document.device_model)
            .bind(&document.serial_number)
            .bind(&document.issue_description)
            .bind(&document.promised_ready_at)
            .bind(&document.status)
            .bind(document.estimated_cost_cents)
            .bind(document.material_cost_cents)
            .bind(&document.assigned_employee_id)
            .bind(&document.assigned_employee_name)
            .bind(&document.split_type)
            .bind(document.split_value)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&document.updated_by_device)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
