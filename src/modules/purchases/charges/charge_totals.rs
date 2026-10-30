/// What the receipt's charges come to, split by who is paid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChargeTotals {
    /// On the supplier's bill, discounts taken off. Changes what is owed to the supplier.
    pub supplier_billed_cents: i64,
    /// Paid to someone else. Never owed to the supplier.
    pub paid_to_others_cents: i64,
}
