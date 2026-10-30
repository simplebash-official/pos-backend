use super::credits::credits;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::purchases::model::SupplierCreditDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts a credit (or a voiding entry) and backfills its `id`.
pub(crate) async fn insert_credit(
    unit: impl Into<Unit<'_>>,
    mut document: SupplierCreditDocument,
) -> AppResult<SupplierCreditDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = credits(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO supplier_credits (
                    key, id, supplier_key, receipt_key, amount_cents, reason, note, reverses_key,
                    recorded_by_user_id, recorded_by_name_snapshot, version, created_at,
                    updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.supplier_key)
            .bind(&document.receipt_key)
            .bind(document.amount_cents)
            .bind(document.reason.as_str())
            .bind(&document.note)
            .bind(&document.reverses_key)
            .bind(&document.recorded_by_user_id)
            .bind(&document.recorded_by_name_snapshot)
            .bind(document.version)
            .bind(bson_to_iso(&document.created_at))
            .bind(bson_to_iso(&document.updated_at))
            .bind(&document.updated_by_device)
            .execute(&mut *conn)
            .await?;
            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
