use super::customers::customers;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;
use crate::modules::customers::model::CustomerDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_customer(
    db: &Db,
    mut document: CustomerDocument,
) -> AppResult<CustomerDocument> {
    match db {
        Db::Mongo(db) => {
            let result = customers(db).insert_one(&document).await?;
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
            let tags_json =
                serde_json::to_string(&document.tags).unwrap_or_else(|_| "[]".to_string());
            let created_at = bson_to_iso(&document.created_at);
            let updated_at = bson_to_iso(&document.updated_at);
            let deleted_at = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO customers (
                    key, id, name, contact_person, primary_phone, secondary_phone,
                    email, address, tags, notes, outstanding_balance_cents,
                    total_purchases_cents, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.name)
            .bind(&document.contact_person)
            .bind(&document.primary_phone)
            .bind(&document.secondary_phone)
            .bind(&document.email)
            .bind(&document.address)
            .bind(&tags_json)
            .bind(&document.notes)
            .bind(document.outstanding_balance_cents)
            .bind(document.total_purchases_cents)
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
