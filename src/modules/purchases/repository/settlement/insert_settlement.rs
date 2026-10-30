use super::settlements::settlements;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::purchases::model::ReceiptSettlementDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts a settlement and backfills its `id`.
pub(crate) async fn insert_settlement(
    unit: impl Into<Unit<'_>>,
    mut document: ReceiptSettlementDocument,
) -> AppResult<ReceiptSettlementDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = settlements(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO receipt_settlements (
                    key, id, receipt_key, supplier_key, amount_cents, payment_kind, payment_key,
                    version, created_at, updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.receipt_key)
            .bind(&document.supplier_key)
            .bind(document.amount_cents)
            .bind(document.payment_kind.as_str())
            .bind(&document.payment_key)
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
