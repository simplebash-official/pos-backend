use super::payments::payments;
use crate::clients::db::Db;
use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::core::error::AppResult;

use crate::modules::billing::model::PaymentDocument;

use mongodb::bson::oid::ObjectId;

pub(crate) async fn insert_payment(
    db: &Db,
    mut document: PaymentDocument,
) -> AppResult<PaymentDocument> {
    match db {
        Db::Mongo(db) => {
            let result = payments(db).insert_one(&document).await?;
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
            let recorded_at_iso = bson_to_iso(&document.recorded_at);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);

            sqlx::query(
                r#"
                INSERT INTO payments (
                    key, id, invoice_key, session_key, amount_cents, payment_method, notes,
                    recorded_by_user_id, recorded_by_name_snapshot, recorded_at,
                    version, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.invoice_key)
            .bind(&document.session_key)
            .bind(document.amount_cents)
            .bind(&document.payment_method)
            .bind(&document.notes)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
            .bind(&recorded_at_iso)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .execute(pool)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
