use serde::{Deserialize, Serialize};

use crate::error::{CalcError, CalcResult};

/// Margins are per mille (255 is 25.5%) so they stay whole numbers.
const PER_MILLE: i128 = 1000;

/// `a / b` rounded half up (towards +infinity), the same as JavaScript's `Math.round`.
fn round_half_up(numerator: i128, denominator: i128) -> i128 {
    (2 * numerator + denominator).div_euclid(2 * denominator)
}

fn ceil_div(numerator: i128, denominator: i128) -> i128 {
    (numerator + denominator - 1).div_euclid(denominator)
}

/// What a line earns at a selling price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
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
) -> CalcResult<Option<LineMargin>> {
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

/// The lowest selling price each that earns at least `margin_tenths` on the lot, rounded up to a
/// whole `step_cents` (1 means no rounding). `None` when the margin is not from 0 up to but not
/// including 100%, or there is no cost or quantity yet.
pub fn suggested_price(
    landed_total_cents: i64,
    quantity: i64,
    margin_tenths: i64,
    step_cents: i64,
) -> CalcResult<Option<i64>> {
    if quantity < 1 || landed_total_cents <= 0 {
        return Ok(None);
    }
    if !(0..PER_MILLE as i64).contains(&margin_tenths) {
        return Ok(None);
    }
    if step_cents < 1 {
        return Err(CalcError::validation(
            "The rounding step must be at least 1",
        ));
    }
    let lowest = ceil_div(
        i128::from(landed_total_cents) * PER_MILLE,
        i128::from(quantity) * (PER_MILLE - i128::from(margin_tenths)),
    );
    let step = i128::from(step_cents);
    Ok(Some(narrow(ceil_div(lowest, step) * step)?))
}

fn narrow(value: i128) -> CalcResult<i64> {
    i64::try_from(value).map_err(|_| CalcError::validation("That amount is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margin_comes_from_the_whole_lot_not_a_rounded_unit_cost() {
        let m = line_margin(5_000_000, 30, 200_000).unwrap().unwrap();
        assert_eq!(m.profit_each_cents, 33_333);
        assert_eq!(m.lot_profit_cents, 1_000_000);
        assert_eq!(m.margin_tenths, 167);
        assert!(!m.below_cost);
    }

    #[test]
    fn a_loss_is_flagged() {
        let m = line_margin(10_000, 10, 800).unwrap().unwrap();
        assert_eq!(m.profit_each_cents, -200);
        assert_eq!(m.lot_profit_cents, -2_000);
        assert_eq!(m.margin_tenths, -250);
        assert!(m.below_cost);
    }

    #[test]
    fn rounding_halves_go_up_like_math_round() {
        assert_eq!(round_half_up(5, 2), 3);
        assert_eq!(round_half_up(-5, 2), -2);
        assert_eq!(round_half_up(-7, 2), -3);
        assert_eq!(round_half_up(4, 2), 2);
    }

    #[test]
    fn no_margin_without_quantity_cost_and_price() {
        assert_eq!(line_margin(0, 10, 100).unwrap(), None);
        assert_eq!(line_margin(1_000, 0, 100).unwrap(), None);
        assert_eq!(line_margin(1_000, 10, 0).unwrap(), None);
    }

    #[test]
    fn suggested_price_rounds_up_to_the_step() {
        let price = |step| suggested_price(8_500_000, 100, 250, step).unwrap();
        assert_eq!(price(1000), Some(114_000));
        assert_eq!(price(100), Some(113_400));
        assert_eq!(price(5000), Some(115_000));
        assert_eq!(price(10000), Some(120_000));
    }

    #[test]
    fn dont_round_gives_the_exact_price() {
        assert_eq!(suggested_price(40_000, 4, 454, 1).unwrap(), Some(18_316));
        assert_eq!(suggested_price(40_000, 4, 454, 100).unwrap(), Some(18_400));
    }

    #[test]
    fn the_price_earns_at_least_the_margin_and_one_cent_less_does_not() {
        for (landed, qty, tenths) in [
            (5_000_000, 30, 300),
            (40_000, 4, 454),
            (999, 7, 1),
            (1, 1, 999),
        ] {
            let price = suggested_price(landed, qty, tenths, 1).unwrap().unwrap();
            let earned = line_margin(landed, qty, price).unwrap().unwrap();
            assert!(earned.margin_tenths >= tenths, "{landed} {qty} {tenths}");
            if price > 1 {
                let lower = i128::from(price - 1) * i128::from(qty) - i128::from(landed);
                let lower_revenue = i128::from(price - 1) * i128::from(qty);
                assert!(lower * PER_MILLE < i128::from(tenths) * lower_revenue);
            }
        }
    }

    #[test]
    fn a_margin_that_cannot_be_earned_or_a_missing_cost_gives_nothing() {
        assert_eq!(suggested_price(8_500_000, 100, 1000, 100).unwrap(), None);
        assert_eq!(suggested_price(8_500_000, 100, -1, 100).unwrap(), None);
        assert_eq!(suggested_price(0, 100, 250, 100).unwrap(), None);
        assert_eq!(suggested_price(8_500_000, 0, 250, 100).unwrap(), None);
    }

    #[test]
    fn a_step_below_one_is_refused() {
        assert!(suggested_price(1_000, 1, 100, 0).is_err());
    }
}
