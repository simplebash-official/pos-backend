use super::stock_batches::stock_batches;

use crate::clients::sqlite::{bson_to_iso, generate_id_hex};

use crate::clients::uow::{Backend, Unit};
use crate::core::error::AppResult;
use crate::modules::inventory::model::StockBatchDocument;

use mongodb::bson::oid::ObjectId;

/// Inserts a new batch and returns it with its id filled in.
pub(crate) async fn insert_batch(
    unit: impl Into<Unit<'_>>,
    mut batch: StockBatchDocument,
) -> AppResult<StockBatchDocument> {
    let mut unit = unit.into();
    match unit.backend().await? {
        Backend::Mongo(db, session) => {
            let result = stock_batches(db).insert_one_in(&batch, session).await?;
            batch.id = result.inserted_id.as_object_id();
            Ok(batch)
        }
        Backend::Sqlite(mut conn) => {
            let id = batch
                .id
                .map(|oid| oid.to_hex())
                .unwrap_or_else(generate_id_hex);
            sqlx::query(
                r#"
                INSERT INTO stock_batches (
                    key, id, product_key, source, purchase_key, supplier_key,
                    received_quantity, remaining_quantity, unit_cost_cents,
                    selling_price_cents, received_at, version, created_at, updated_at,
                    deleted_at, updated_by_device
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&batch.key)
            .bind(&id)
            .bind(&batch.product_key)
            .bind(batch.source.as_str())
            .bind(&batch.purchase_key)
            .bind(&batch.supplier_key)
            .bind(batch.received_quantity)
            .bind(batch.remaining_quantity)
            .bind(batch.unit_cost_cents)
            .bind(batch.selling_price_cents)
            .bind(bson_to_iso(&batch.received_at))
            .bind(batch.version)
            .bind(bson_to_iso(&batch.created_at))
            .bind(bson_to_iso(&batch.updated_at))
            .bind(batch.deleted_at.as_ref().map(bson_to_iso))
            .bind(&batch.updated_by_device)
            .execute(&mut *conn)
            .await?;
            batch.id = ObjectId::parse_str(&id).ok();
            Ok(batch)
        }
    }
}
