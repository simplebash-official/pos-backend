use super::batch_not_found::batch_not_found;
use super::new_batch::NewBatch;
use super::validate_batch_prices::validate_batch_prices;

use crate::clients::uow::Unit;
use crate::core::constants::prefixes;
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::inventory::StockBatch;
use crate::modules::inventory::model::StockBatchDocument;
use crate::modules::inventory::repository;
use mongodb::bson::DateTime as BsonDateTime;

/// Creates a batch and brings its units in. The batch is inserted empty and
/// its units arrive through `apply_stock_delta`, so the batch's remaining
/// quantity, the product total and the ledger agree from the first write —
/// the same shape every later move has, which is what lets sync rebuild the
/// remaining quantity from movements alone.
pub(crate) async fn create_batch(
    unit: impl Into<Unit<'_>>,
    batch: NewBatch,
) -> AppResult<StockBatch> {
    let mut unit = unit.into();
    if batch.quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    validate_batch_prices(batch.unit_cost_cents, batch.selling_price_cents)?;

    let now = BsonDateTime::now();
    let inserted = repository::stock_batch::insert_batch(
        &mut unit,
        StockBatchDocument {
            id: None,
            key: generate_id(prefixes::STOCK_BATCH),
            product_key: batch.product_key,
            source: batch.source,
            purchase_key: batch.purchase_key,
            supplier_key: batch.supplier_key,
            received_quantity: batch.quantity,
            remaining_quantity: 0,
            unit_cost_cents: batch.unit_cost_cents,
            selling_price_cents: batch.selling_price_cents,
            received_at: batch.received_at,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        },
    )
    .await?;

    super::super::stock::apply_stock_delta(
        &mut unit,
        batch.product_id,
        Some(&inserted.key),
        batch.quantity,
        batch.movement_type,
        batch.reference_id,
        batch.note,
    )
    .await?;

    let stored = repository::stock_batch::find_batch_by_key(&mut unit, &inserted.key)
        .await?
        .ok_or_else(|| batch_not_found(&inserted.key))?;
    Ok(stored.into_stock_batch())
}
