use super::narrow::narrow;
use super::per_mille::PER_MILLE;
use crate::core::error::{AppError, AppResult};

fn ceil_div(numerator: i128, denominator: i128) -> i128 {
    (numerator + denominator - 1).div_euclid(denominator)
}

/// The lowest selling price each that earns at least `margin_tenths` on the lot, rounded up to a
/// whole `step_cents` (1 means no rounding). `None` when the margin is not from 0 up to but not
/// including 100%, or there is no cost or quantity yet.
pub fn suggested_price(
    landed_total_cents: i64,
    quantity: i64,
    margin_tenths: i64,
    step_cents: i64,
) -> AppResult<Option<i64>> {
    if quantity < 1 || landed_total_cents <= 0 {
        return Ok(None);
    }
    if !(0..PER_MILLE as i64).contains(&margin_tenths) {
        return Ok(None);
    }
    if step_cents < 1 {
        return Err(AppError::validation("The rounding step must be at least 1"));
    }
    let lowest = ceil_div(
        i128::from(landed_total_cents) * PER_MILLE,
        i128::from(quantity) * (PER_MILLE - i128::from(margin_tenths)),
    );
    let step = i128::from(step_cents);
    Ok(Some(narrow(ceil_div(lowest, step) * step)?))
}
