/// The landed total per unit to the nearest cent, for display. Stock is costed with `split_cost`.
#[must_use]
pub fn rounded_unit(total_cents: i64, quantity: i64) -> i64 {
    if quantity <= 0 {
        return 0;
    }
    (total_cents + quantity / 2).div_euclid(quantity)
}
