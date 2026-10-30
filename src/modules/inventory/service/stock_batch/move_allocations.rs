use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{BatchAllocation, StockMovementType};

use crate::modules::inventory::repository;

/// Moves `allocations` in (`sign = 1`) or out (`sign = -1`) of their batches,
/// one movement per batch, all tagged with `movement_type`/`reference_id` —
/// how sales, voids, returns and exchanges touch stock. Stops at the first
/// failure and returns it; callers decide whether that fails the request.
pub(crate) async fn move_allocations(
    db: &Db,
    product_key: &str,
    allocations: &[BatchAllocation],
    sign: i64,
    movement_type: StockMovementType,
    reference_id: &str,
    note: &str,
) -> AppResult<()> {
    let product = repository::product::find_product_by_key(db, product_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;
    let product_id = product
        .id
        .expect("persisted product document must have an id");
    for allocation in allocations {
        super::super::stock::apply_stock_delta(
            db,
            product_id,
            Some(&allocation.batch_key),
            sign * allocation.quantity,
            movement_type,
            Some(reference_id.to_string()),
            Some(note.to_string()),
        )
        .await?;
    }
    Ok(())
}
