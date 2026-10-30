use super::line_margin_fn::line_margin;
use super::per_mille::PER_MILLE;
use super::round_half_up::round_half_up;
use super::suggested_price::suggested_price;

#[test]
fn margin_comes_from_the_whole_lot_not_a_rounded_unit_cost() {
    // 30 pieces for Rs 50,000.00, sold at Rs 2,000.00 each.
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
    // Cost Rs 850.00 each: 25% needs Rs 1,133.34.
    let price = |step| suggested_price(8_500_000, 100, 250, step).unwrap();
    assert_eq!(price(1000), Some(114_000));
    assert_eq!(price(100), Some(113_400));
    assert_eq!(price(5000), Some(115_000));
    assert_eq!(price(10000), Some(120_000));
}

#[test]
fn dont_round_gives_the_exact_price() {
    // Rs 100 each at 45.4% needs Rs 183.16; Rs 1 rounding makes it Rs 184.
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
