use super::purchases::purchases;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::purchases::model::PurchaseDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts `document` and backfills its `id` from the driver-generated
/// `_id`, since the caller builds the document with `id: None`.
pub(crate) async fn insert_purchase(
    unit: impl Into<Unit<'_>>,
    mut document: PurchaseDocument,
) -> AppResult<PurchaseDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = purchases(db).insert_one_in(&document, session).await?;
            document.id = Some(
                result
                    .inserted_id
                    .as_object_id()
                    .expect("inserted_id is always an ObjectId for an auto-generated _id"),
            );
            Ok(document)
        }
        Backend::Sqlite(mut conn) => {
            let id = document
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            let date_iso = bson_to_iso(&document.date);
            let created_at_iso = bson_to_iso(&document.created_at);
            let updated_at_iso = bson_to_iso(&document.updated_at);
            let deleted_at_iso = document.deleted_at.as_ref().map(bson_to_iso);

            sqlx::query(
                r#"
                INSERT INTO purchases (
                    key, id, supplier_key, product_key, quantity, line_total_cents,
                    charges_share_cents, selling_price_cents, date, receipt_key, version,
                    created_at, updated_at, deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&document.key)
            .bind(&id)
            .bind(&document.supplier_key)
            .bind(&document.product_key)
            .bind(document.quantity)
            .bind(document.line_total_cents)
            .bind(document.charges_share_cents)
            .bind(document.selling_price_cents)
            .bind(&date_iso)
            .bind(&document.receipt_key)
            .bind(document.version)
            .bind(&created_at_iso)
            .bind(&updated_at_iso)
            .bind(&deleted_at_iso)
            .bind(&document.updated_by_device)
            .execute(&mut *conn)
            .await?;

            document.id = ObjectId::parse_str(&id).ok();
            Ok(document)
        }
    }
}
