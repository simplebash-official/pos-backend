use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::BatchAllocation;
use crate::modules::inventory::model::StockBatchDocument;

/// Splits a sale of `quantity` units at `selling_price_cents` across the
/// in-stock batches of that price, oldest first. Pure, so the rule is tested
/// without a database. Errors when the tier doesn't exist
/// (`PRICE_TIER_UNAVAILABLE`) or holds fewer units than asked
/// (`INSUFFICIENT_STOCK`), before anything is written.
pub(crate) fn plan_from_batches<'a>(
    batches: impl IntoIterator<Item = &'a StockBatchDocument>,
    product_name: &str,
    selling_price_cents: i64,
    quantity: i64,
) -> AppResult<Vec<BatchAllocation>> {
    if quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    let tier: Vec<&StockBatchDocument> = batches
        .into_iter()
        .filter(|batch| {
            batch.selling_price_cents == selling_price_cents && batch.remaining_quantity > 0
        })
        .collect();
    if tier.is_empty() {
        return Err(AppError::conflict_with_details(
            codes::PRICE_TIER_UNAVAILABLE,
            format!("{product_name} is not in stock at this price any more."),
            serde_json::json!({ "sellingPriceCents": selling_price_cents }),
        ));
    }
    let available: i64 = tier.iter().map(|batch| batch.remaining_quantity).sum();
    if available < quantity {
        return Err(AppError::conflict_with_details(
            codes::INSUFFICIENT_STOCK,
            format!(
                "Only {available} unit{} of {product_name} {} left at this price (asked for {quantity}).",
                if available == 1 { "" } else { "s" },
                if available == 1 { "is" } else { "are" },
            ),
            serde_json::json!({
                "available": available,
                "requested": quantity,
                "sellingPriceCents": selling_price_cents,
            }),
        ));
    }

    let mut left = quantity;
    let mut allocations = Vec::new();
    for batch in tier {
        if left == 0 {
            break;
        }
        let take = left.min(batch.remaining_quantity);
        allocations.push(BatchAllocation {
            batch_key: batch.key.clone(),
            quantity: take,
            unit_cost_cents: batch.unit_cost_cents,
        });
        left -= take;
    }
    Ok(allocations)
}
