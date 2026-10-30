use crate::domain::purchase_receipts::Allocation;

/// The weight each line carries when a charge is spread: what it costs, or how many units it has.
#[must_use]
pub fn weights(allocation: Allocation, lines: &[(i64, i64)]) -> Vec<i64> {
    lines
        .iter()
        .map(|&(quantity, line_total_cents)| match allocation {
            Allocation::Value => line_total_cents,
            Allocation::Quantity => quantity,
        })
        .collect()
}
