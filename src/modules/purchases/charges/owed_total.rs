use super::charge_totals::ChargeTotals;

/// What a receipt owes the supplier before credits and payments: the goods plus the charges the
/// supplier billed. The one definition behind the receipt total and the supplier balance.
#[must_use]
pub fn owed_total(goods_cents: i64, totals: ChargeTotals) -> i64 {
    goods_cents + totals.supplier_billed_cents
}
