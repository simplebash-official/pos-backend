use super::line_margin::LineMargin;
use super::narrow::narrow;
use super::per_mille::PER_MILLE;
use super::round_half_up::round_half_up;
use crate::core::error::AppResult;

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
