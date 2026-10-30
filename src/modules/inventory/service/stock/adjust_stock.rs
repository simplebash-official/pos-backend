use super::apply_stock_delta::apply_stock_delta;
use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{
    BatchSource, StockAdjustmentRequest, StockAdjustmentResponse, StockMovementType,
};

use crate::modules::inventory::repository;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;

/// Manual stock adjustment via `PATCH /products/{id}/stock`. A negative
/// delta takes units out of the named batch; a positive delta brings units
/// in as a new `adjustment` batch with its own cost and selling price (stock
/// never exists outside a batch). Echoes back the pre-adjustment quantity so
/// a client can display the change without a second request.
pub(crate) async fn adjust_stock(
    db: &Db,
    id: ObjectId,
    body: StockAdjustmentRequest,
) -> AppResult<StockAdjustmentResponse> {
    crate::core::logging::domain::tracked("inventory.stock_adjusted", async move {
        let product = repository::product::find_product_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;

        let (previous_stock_quantity, updated) = match body.delta {
            0 => return Err(AppError::validation("Adjustment delta cannot be 0")),
            delta if delta < 0 => {
                let batch_key = body.batch_key.as_deref().ok_or_else(|| {
                    AppError::validation("Taking stock out needs the batchKey it comes from")
                })?;
                let batch = repository::stock_batch::find_batch_by_key(db, batch_key)
                    .await?
                    .filter(|batch| batch.product_key == product.key)
                    .ok_or_else(|| super::super::stock_batch::batch_not_found(batch_key))?;
                apply_stock_delta(
                    db,
                    id,
                    Some(&batch.key),
                    delta,
                    StockMovementType::ManualAdjustment,
                    None,
                    body.reason,
                )
                .await?
            }
            delta => {
                let (Some(unit_cost_cents), Some(selling_price_cents)) =
                    (body.unit_cost_cents, body.selling_price_cents)
                else {
                    return Err(AppError::validation(
                        "Adding stock needs the unitCostCents and sellingPriceCents of the new units",
                    ));
                };
                let previous = product.stock_quantity;
                super::super::stock_batch::create_batch(
                    db,
                    super::super::stock_batch::NewBatch {
                        product_id: id,
                        product_key: product.key.clone(),
                        source: BatchSource::Adjustment,
                        purchase_key: None,
                        supplier_key: None,
                        quantity: delta,
                        unit_cost_cents,
                        selling_price_cents,
                        received_at: BsonDateTime::now(),
                        movement_type: StockMovementType::ManualAdjustment,
                        reference_id: None,
                        note: body.reason,
                    },
                )
                .await?;
                let updated = repository::product::find_product_by_id(db, id)
                    .await?
                    .expect("product was just adjusted and must exist");
                (previous, updated)
            }
        };

        Ok(StockAdjustmentResponse {
            id: updated
                .id
                .expect("persisted product document must have an id")
                .to_hex(),
            key: updated.key,
            sku: updated.sku,
            name: updated.name,
            stock_quantity: updated.stock_quantity,
            previous_stock_quantity,
            delta: body.delta,
            updated_at: updated.updated_at.to_chrono(),
        })
    })
    .await
}
