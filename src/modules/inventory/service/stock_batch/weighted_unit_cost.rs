use crate::domain::inventory::BatchAllocation;

/// Weighted unit cost of an allocation set, floored — what an invoice line
/// records as its `unit_cost_cents` so the profit reports read the real cost.
pub(crate) fn weighted_unit_cost(allocations: &[BatchAllocation]) -> i64 {
    let units: i64 = allocations.iter().map(|a| a.quantity).sum();
    if units == 0 {
        return 0;
    }
    allocations
        .iter()
        .map(|a| a.quantity * a.unit_cost_cents)
        .sum::<i64>()
        / units
}
