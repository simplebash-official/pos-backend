use crate::core::error::{AppError, AppResult};

/// What a line earns at a selling price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineMargin {
    /// Selling price less the cost of one unit, to the nearest cent.
    pub profit_each_cents: i64,
    /// What the whole lot earns if every unit sells at the price. Negative is a loss.
    pub lot_profit_cents: i64,
    /// Profit as a share of the selling price in tenths of a percent (167 is 16.7%).
    pub margin_tenths: i64,
    pub below_cost: bool,
}

/// `None` until there is a quantity, a cost and a selling price to compare.
pub fn line_margin(
    landed_total_cents: i64,
    quantity: i64,
    selling_price_cents: i64,
) -> AppResult<Option<LineMargin>> {
    if quantity < 1 || landed_total_cents <= 0 || selling_price_cents <= 0 {
        return Ok(None);
    }
    let revenue = i128::from(selling_price_cents) * i128::from(quantity);
    let lot_profit = revenue - i128::from(landed_total_cents);
    Ok(Some(LineMargin {
        profit_each_cents: narrow(round_half_up(lot_profit, i128::from(quantity)))?,
        lot_profit_cents: narrow(lot_profit)?,
        margin_tenths: narrow(round_half_up(lot_profit * PER_MILLE, revenue))?,
        below_cost: lot_profit < 0,
    }))
}

pub(super) fn narrow(value: i128) -> AppResult<i64> {
    i64::try_from(value).map_err(|_| AppError::validation("That amount is too large"))
}

/// Margins are per mille (255 is 25.5%) so they stay whole numbers.
pub(super) const PER_MILLE: i128 = 1000;

/// `a / b` rounded half up (towards +infinity), the same as JavaScript's `Math.round`.
pub(super) fn round_half_up(numerator: i128, denominator: i128) -> i128 {
    (2 * numerator + denominator).div_euclid(2 * denominator)
}

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
