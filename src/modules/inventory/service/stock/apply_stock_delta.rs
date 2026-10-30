use crate::clients::uow::Unit;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::inventory::StockMovementType;
use crate::modules::inventory::model::{ProductDocument, StockMovementDocument};
use crate::modules::inventory::repository;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;

/// Applies a signed `delta` to one batch of a product and to the product's
/// total, and records the change as a `StockMovement` carrying the batch key
/// — the three are always written together, never independently (see the
/// module banner). Every unit of stock lives in a batch, so a quantity change
/// always names one; only the audit-only return movements (`delta == 0`)
/// carry no batch. Rejects with `INSUFFICIENT_STOCK` rather than clamping to
/// zero, since a delta larger than what the batch holds is almost always a
/// bug (e.g. a double-submitted sale) that should surface as an error. The
/// batch is moved first because its guard is the stricter one; if the
/// product write then fails, the batch move is undone so the two never
/// disagree. This is the one place stock is ever mutated, so every caller
/// gets the same guard and audit trail for free.
pub(crate) async fn apply_stock_delta(
    unit: impl Into<Unit<'_>>,
    id: ObjectId,
    batch_key: Option<&str>,
    delta: i64,
    movement_type: StockMovementType,
    reference_id: Option<String>,
    note: Option<String>,
) -> AppResult<(i64, ProductDocument)> {
    let mut unit = unit.into();
    if delta != 0 && batch_key.is_none() {
        return Err(AppError::internal(
            "A stock quantity change must name the batch it moves",
        ));
    }

    if let Some(batch_key) = batch_key
        && delta != 0
        && repository::stock_batch::adjust_batch_remaining(&mut unit, batch_key, delta)
            .await?
            .is_none()
    {
        let batch = repository::stock_batch::find_batch_by_key(&mut unit, batch_key)
            .await?
            .ok_or_else(|| super::super::stock_batch::batch_not_found(batch_key))?;
        let name = repository::product::find_product_by_id(&mut unit, id)
            .await?
            .map(|product| product.name)
            .unwrap_or_default();
        return Err(insufficient_stock(
            batch.remaining_quantity,
            -delta,
            &name,
            serde_json::json!({
                "available": batch.remaining_quantity,
                "requested": -delta,
                "productId": id.to_hex(),
                "batchKey": batch_key,
                "sellingPriceCents": batch.selling_price_cents,
            }),
        ));
    }

    let adjusted = repository::product::adjust_product_stock_pipeline(&mut unit, id, delta).await;
    let (previous_stock_quantity, updated) = match adjusted {
        Ok(Some(result)) => result,
        failed => {
            if let Some(batch_key) = batch_key
                && delta != 0
            {
                repository::stock_batch::adjust_batch_remaining(&mut unit, batch_key, -delta)
                    .await?;
            }
            failed?;
            let existing = repository::product::find_product_by_id(&mut unit, id)
                .await?
                .ok_or_else(|| {
                    AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
                })?;
            return Err(insufficient_stock(
                existing.stock_quantity,
                -delta,
                &existing.name,
                serde_json::json!({
                    "available": existing.stock_quantity,
                    "requested": -delta,
                    "productId": id.to_hex(),
                }),
            ));
        }
    };

    let now = BsonDateTime::now();
    repository::stock::insert_stock_movement(
        &mut unit,
        StockMovementDocument {
            id: None,
            key: generate_id(prefixes::STOCK_MOVEMENT),
            product_id: id,
            batch_key: batch_key.map(str::to_string),
            quantity_delta: delta,
            movement_type,
            reference_id,
            note,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        },
    )
    .await?;

    Ok((previous_stock_quantity, updated))
}

fn insufficient_stock(
    available: i64,
    requested: i64,
    name: &str,
    details: serde_json::Value,
) -> AppError {
    let message = format!(
        "Only {available} unit{} of {name} {} left in stock (asked for {requested}).",
        if available == 1 { "" } else { "s" },
        if available == 1 { "is" } else { "are" },
    );
    AppError::conflict_with_details(codes::INSUFFICIENT_STOCK, message, details)
}
