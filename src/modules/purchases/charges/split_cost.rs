use super::batch_cost::BatchCost;

/// Splits a line's landed total over its units so the batches add up to it exactly. If the total
/// divides evenly it is one batch; otherwise `remainder` units cost one cent more than the rest
/// (two batches, the cheaper one first). `quantity` must be at least 1 and `total_cents` positive.
#[must_use]
pub fn split_cost(total_cents: i64, quantity: i64) -> Vec<BatchCost> {
    let each = total_cents / quantity;
    let remainder = total_cents % quantity;
    if remainder == 0 {
        vec![BatchCost {
            quantity,
            unit_cost_cents: each,
        }]
    } else {
        vec![
            BatchCost {
                quantity: quantity - remainder,
                unit_cost_cents: each,
            },
            BatchCost {
                quantity: remainder,
                unit_cost_cents: each + 1,
            },
        ]
    }
}
