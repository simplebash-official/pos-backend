use super::print_jobs::print_jobs;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;
use crate::modules::print_jobs::model::PrintJobDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert(db: &Db, mut document: PrintJobDocument) -> AppResult<PrintJobDocument> {
    match db {
        Db::Mongo(db) => {
            let result = print_jobs(db).insert_one(&document).await?;
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
                INSERT INTO print_jobs (
                    key, id, ticket_number, customer_key, customer_name, customer_phone,
                    job_type, quantity, promised_ready_at, status, estimated_cost_cents,
                    material_cost_cents, assigned_employee_id, assigned_employee_name,
                    split_type, split_value, version, created_at, updated_at, deleted_at,
                    updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.ticket_number)
            .bind(&document.customer_key)
            .bind(&document.customer_name)
            .bind(&document.customer_phone)
            .bind(&document.job_type)
            .bind(document.quantity as i64)
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
