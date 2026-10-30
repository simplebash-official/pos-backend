use super::get_batch_for_product::get_batch_for_product;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::BatchAllocation;

/// Price and batch allocations of a serialized line: each unit sells at its
/// own batch's price, so the chosen serials must all come from batches at
/// one price (a cart line has one unit price). `batch_keys` is each chosen
/// serial's batch, one entry per unit.
pub(crate) async fn allocations_for_serials(
    db: &Db,
    product_key: &str,
    product_name: &str,
    batch_keys: &[String],
) -> AppResult<(i64, Vec<BatchAllocation>)> {
    let mut allocations: Vec<BatchAllocation> = Vec::new();
    let mut price: Option<i64> = None;
    for batch_key in batch_keys {
        if let Some(existing) = allocations.iter_mut().find(|a| &a.batch_key == batch_key) {
            existing.quantity += 1;
            continue;
        }
        let batch = get_batch_for_product(db, batch_key, product_key).await?;
        match price {
            Some(p) if p != batch.selling_price_cents => {
                return Err(AppError::validation(format!(
                    "The chosen units of {product_name} sell at different prices — put each price on its own line"
                )));
            }
            _ => price = Some(batch.selling_price_cents),
        }
        allocations.push(BatchAllocation {
            batch_key: batch.key,
            quantity: 1,
            unit_cost_cents: batch
                .unit_cost_cents
                .expect("a batch read from the store carries its cost"),
        });
    }
    let price =
        price.ok_or_else(|| AppError::validation("A serialized line needs its serial numbers"))?;
    Ok((price, allocations))
}
