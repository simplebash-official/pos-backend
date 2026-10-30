use crate::domain::inventory::BatchAllocation;

/// Which batches `quantity` returned units of a sold line go back into: the
/// line's own batches, newest allocation first, skipping the units earlier
/// returns already put back (`already_returned`). Pure and deterministic, so
/// a later return of the same line continues where the previous one stopped.
pub(crate) fn allocations_for_return(
    line_allocations: &[BatchAllocation],
    already_returned: i64,
    quantity: i64,
) -> Vec<BatchAllocation> {
    let mut skip = already_returned.max(0);
    let mut left = quantity;
    let mut result = Vec::new();
    for allocation in line_allocations.iter().rev() {
        if left == 0 {
            break;
        }
        let usable = allocation.quantity - skip.min(allocation.quantity);
        skip -= skip.min(allocation.quantity);
        if usable == 0 {
            continue;
        }
        let take = usable.min(left);
        result.push(BatchAllocation {
            batch_key: allocation.batch_key.clone(),
            quantity: take,
            unit_cost_cents: allocation.unit_cost_cents,
        });
        left -= take;
    }
    result
}
