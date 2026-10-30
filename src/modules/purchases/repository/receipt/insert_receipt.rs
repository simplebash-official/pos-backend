use super::receipts::receipts;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseReceiptDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts a receipt and backfills its `id`.
pub(crate) async fn insert_receipt(
    unit: impl Into<Unit<'_>>,
    mut document: PurchaseReceiptDocument,
) -> AppResult<PurchaseReceiptDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = receipts(db).insert_one_in(&document, session).await?;
            document.id = result.inserted_id.as_object_id();
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                "INSERT INTO purchase_receipts (
                    key, id, receipt_number, supplier_key, reference_no, date, notes,
                    version, created_at, updated_at, updated_by_device
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.receipt_number)
            .bind(&document.supplier_key)
            .bind(&document.reference_no)
            .bind(bson_to_iso(&document.date))
            .bind(&document.notes)
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
