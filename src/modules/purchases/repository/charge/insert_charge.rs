use super::charges::charges;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;

use crate::modules::purchases::model::ReceiptChargeDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts a charge and backfills its `id`.
pub(crate) async fn insert_charge(
    unit: impl Into<Unit<'_>>,
    mut document: ReceiptChargeDocument,
) -> AppResult<ReceiptChargeDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = charges(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO receipt_charges (
                    key, id, receipt_key, supplier_key, date, kind, amount_cents, billed_by,
                    allocation, payee_key, description, payment_kind, payment_key,
                    version, created_at, updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.receipt_key)
            .bind(&document.supplier_key)
            .bind(bson_to_iso(&document.date))
            .bind(document.kind.as_str())
            .bind(document.amount_cents)
            .bind(document.billed_by.as_str())
            .bind(document.allocation.map(|a| a.as_str()))
            .bind(&document.payee_key)
            .bind(&document.description)
            .bind(document.payment_kind.map(|k| k.as_str()))
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
