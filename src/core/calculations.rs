// Centralized monetary & financial calculations engine for simplebash-pos backend.
// The one copy of the POS maths: the sale save path and the live cart/payment preview
// (`modules::calc::checkout`) both call it, so a preview can never differ from what is stored.
//
// Provides pure mathematical functions with 100% precision in integer cents:
// - Line item totals & discounts
// - Order-level discounts (percentage with half-up rounding matching frontend, fixed)
// - Sale subtotals, totals, and change due
// - Split payment validation

use crate::{
    core::error::{AppError, AppResult},
    domain::billing::{InvoiceItem, PricingAdjustments, SplitPayment},
};

/// Computes the net line total in cents from unit price, quantity, and discount.
#[inline]
pub fn compute_line_total(unit_price_cents: i64, quantity: i64, discount_cents: i64) -> i64 {
    let gross = unit_price_cents.max(0) * quantity.max(0);
    (gross - discount_cents.max(0)).max(0)
}

/// Computes order discount type, value, and cents against a subtotal using standard
/// half-up rounding for percentage discounts (matching frontend).
pub fn compute_order_discount(
    subtotal_cents: i64,
    adjustments: Option<&PricingAdjustments>,
) -> (String, f64, i64) {
    if subtotal_cents <= 0 {
        return match adjustments {
            Some(adj) => (adj.discount_type.clone(), adj.discount_value, 0),
            None => ("fixed".to_string(), 0.0, 0),
        };
    }

    match adjustments {
        Some(PricingAdjustments {
            discount_type,
            discount_value,
        }) if discount_type == "percentage" => {
            let clamped_pct = discount_value.clamp(0.0, 100.0);
            let discount_cents = percentage_discount_cents(subtotal_cents, clamped_pct);
            (discount_type.clone(), clamped_pct, discount_cents)
        }
        Some(PricingAdjustments {
            discount_type,
            discount_value,
        }) if discount_type == "fixed" => {
            let fixed_cents = (*discount_value as i64).min(subtotal_cents).max(0);
            (discount_type.clone(), *discount_value, fixed_cents)
        }
        Some(PricingAdjustments {
            discount_type,
            discount_value,
        }) => (
            discount_type.clone(),
            *discount_value,
            (*discount_value as i64).min(subtotal_cents).max(0),
        ),
        None => ("fixed".to_string(), 0.0, 0),
    }
}

/// Computes all sale totals: (subtotal_cents, discount_type, discount_value, discount_cents, total_cents).
pub fn compute_sale_totals(
    items: &[InvoiceItem],
    adjustments: Option<&PricingAdjustments>,
) -> (i64, String, f64, i64, i64) {
    let subtotal_cents: i64 = items.iter().map(|item| item.total_cents).sum();
    let (discount_type, discount_value, discount_cents) =
        compute_order_discount(subtotal_cents, adjustments);
    let total_cents = (subtotal_cents - discount_cents).max(0);
    (
        subtotal_cents,
        discount_type,
        discount_value,
        discount_cents,
        total_cents,
    )
}

/// Computes change due in cents if tendered amount was provided.
#[inline]
pub fn compute_change_due(amount_received_cents: Option<i64>, total_cents: i64) -> Option<i64> {
    amount_received_cents.map(|received| (received - total_cents).max(0))
}

/// Validates split payment legs against total due.
pub fn validate_split_payments(total_cents: i64, legs: &[SplitPayment]) -> AppResult<()> {
    if legs.is_empty() {
        return Err(AppError::validation(
            "A split payment must include at least one split leg",
        ));
    }

    for leg in legs {
        if leg.amount_cents < 0 {
            return Err(AppError::validation(
                "Split payment leg amount cannot be negative",
            ));
        }
    }

    let legs_total: i64 = legs.iter().map(|leg| leg.amount_cents).sum();
    if legs_total > total_cents {
        return Err(AppError::validation(
            "Split payment legs cannot sum to more than the invoice total",
        ));
    }

    Ok(())
}

/// `n / d` rounded half up (towards +infinity), the same as JavaScript's `Math.round`.
fn round_half_up(n: i128, d: i128) -> i128 {
    (2 * n + d).div_euclid(2 * d)
}

/// A percentage (0-100, at most two decimals are kept) of `base_cents`, rounded half up and never
/// more than `base_cents`. Done in whole numbers: the percent is turned into basis points once at
/// the edge, so no floating point touches the money.
pub fn percentage_discount_cents(base_cents: i64, percent: f64) -> i64 {
    if base_cents <= 0 {
        return 0;
    }
    let basis_points = (percent.clamp(0.0, 100.0) * 100.0).round() as i128;
    let discount = round_half_up(i128::from(base_cents) * basis_points, 10_000);
    i64::try_from(discount).map_or(base_cents, |d| d.clamp(0, base_cents))
}

/// A part as a share of its whole, in basis points (percent x 100), rounded half up and kept between
/// 0 and the whole: a progress bar or a pie slice never goes past full. A whole of nothing has no
/// share.
pub fn share_basis_points(part_cents: i64, whole_cents: i64) -> i64 {
    if whole_cents <= 0 || part_cents <= 0 {
        return 0;
    }
    let bp = round_half_up(i128::from(part_cents) * 10_000, i128::from(whole_cents));
    i64::try_from(bp).map_or(10_000, |v| v.clamp(0, 10_000))
}

/// What a customer has paid on an invoice and what they still owe. A credit invoice owes its total
/// less what was paid and less what was returned (credit notes), never below zero; a sale paid at
/// the till owes nothing and counts as paid in full.
pub fn invoice_paid_and_balance(
    is_credit: bool,
    total_cents: i64,
    paid_cents: i64,
    refunded_cents: i64,
) -> (i64, i64) {
    if is_credit {
        let balance = total_cents
            .saturating_sub(paid_cents)
            .saturating_sub(refunded_cents)
            .max(0);
        (paid_cents, balance)
    } else {
        (total_cents, 0)
    }
}

/// What a repair or print job earned the shop: the price less what the parts cost, never below zero.
pub fn job_profit_cents(price_cents: i64, material_cost_cents: i64) -> i64 {
    price_cents.saturating_sub(material_cost_cents).max(0)
}

/// What the assigned employee earns on a job. A percentage is taken of the job's profit; a fixed
/// amount (cents) is paid in full but never more than the profit. No split rule means no commission.
pub fn commission_cents(profit_cents: i64, split_type: Option<&str>, split_value: f64) -> i64 {
    match split_type {
        Some("percentage") => percentage_discount_cents(profit_cents, split_value),
        Some("fixed") => (split_value.round() as i64).clamp(0, profit_cents.max(0)),
        _ => 0,
    }
}

/// How a cart line works out: price x quantity, the discount that applies, and what is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineTotals {
    pub gross_cents: i64,
    pub discount_cents: i64,
    pub total_cents: i64,
}

/// A line's gross, discount (capped at the gross; negative counts as none) and net, with overflow
/// refused instead of wrapped.
pub fn line_totals(
    unit_price_cents: i64,
    quantity: i64,
    discount_cents: i64,
) -> AppResult<LineTotals> {
    let gross = unit_price_cents
        .max(0)
        .checked_mul(quantity.max(0))
        .ok_or_else(|| AppError::validation("That line is too large"))?;
    let discount = discount_cents.max(0).min(gross);
    Ok(LineTotals {
        gross_cents: gross,
        discount_cents: discount,
        total_cents: gross - discount,
    })
}

/// The order discount in cents for a discount type ("percentage" or "fixed", a fixed value being
/// cents) against a subtotal. Anything else, or no value, is no discount.
pub fn order_discount_cents(subtotal_cents: i64, discount_type: &str, discount_value: f64) -> i64 {
    if subtotal_cents <= 0 || discount_value <= 0.0 {
        return 0;
    }
    match discount_type {
        "percentage" => percentage_discount_cents(subtotal_cents, discount_value),
        "fixed" => (discount_value.round() as i64).clamp(0, subtotal_cents),
        _ => 0,
    }
}

/// Where a cart line's money came from, for the retail / repairs / print split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSource {
    Retail,
    Repair,
    Print,
}

/// What was typed as a cart line's discount. A percentage follows the line (it is a share of the
/// line's gross whatever the quantity is); a fixed one is cents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineDiscount {
    None,
    Fixed(i64),
    Percentage(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CartLine {
    pub unit_price_cents: i64,
    pub quantity: i64,
    pub discount: LineDiscount,
    pub source: LineSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartTotals {
    pub lines: Vec<LineTotals>,
    pub subtotal_cents: i64,
    pub order_discount_cents: i64,
    pub total_cents: i64,
    pub unit_count: i64,
    pub retail_cents: i64,
    pub repairs_cents: i64,
    pub print_cents: i64,
}

/// Every line, the subtotal, the order discount, the total and the retail/repairs/print split.
pub fn cart_totals(
    lines: &[CartLine],
    discount_type: Option<&str>,
    discount_value: f64,
) -> AppResult<CartTotals> {
    let mut out = CartTotals {
        lines: Vec::with_capacity(lines.len()),
        subtotal_cents: 0,
        order_discount_cents: 0,
        total_cents: 0,
        unit_count: 0,
        retail_cents: 0,
        repairs_cents: 0,
        print_cents: 0,
    };
    for line in lines {
        let gross = line_totals(line.unit_price_cents, line.quantity, 0)?.gross_cents;
        let discount_cents = match line.discount {
            LineDiscount::None => 0,
            LineDiscount::Fixed(cents) => cents,
            LineDiscount::Percentage(percent) => percentage_discount_cents(gross, percent),
        };
        let totals = line_totals(line.unit_price_cents, line.quantity, discount_cents)?;
        let add = |sum: i64| {
            sum.checked_add(totals.total_cents)
                .ok_or_else(|| AppError::validation("The cart is too large"))
        };
        out.subtotal_cents = add(out.subtotal_cents)?;
        out.unit_count = out.unit_count.saturating_add(line.quantity);
        match line.source {
            LineSource::Retail => out.retail_cents = add(out.retail_cents)?,
            LineSource::Repair => out.repairs_cents = add(out.repairs_cents)?,
            LineSource::Print => out.print_cents = add(out.print_cents)?,
        }
        out.lines.push(totals);
    }
    out.order_discount_cents = match discount_type {
        Some(kind) => order_discount_cents(out.subtotal_cents, kind, discount_value),
        None => 0,
    };
    out.total_cents = (out.subtotal_cents - out.order_discount_cents).max(0);
    Ok(out)
}

/// How a sale is being paid, as far as the payment maths cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenderMethod {
    Cash,
    Card,
    Online,
    Split,
}

#[derive(Debug, Clone)]
pub struct PaymentInput<'a> {
    pub total_cents: i64,
    pub tendered_cents: i64,
    pub method: TenderMethod,
    pub split_legs_cents: &'a [i64],
    pub customer_balance_cents: i64,
    pub is_credit: bool,
    /// For a credit sale: what the customer pays up front.
    pub credit_deposit_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentState {
    pub effective_tendered_cents: i64,
    pub change_due_cents: i64,
    pub short_by_cents: i64,
    pub is_cash_short: bool,
    pub is_card_short: bool,
    pub is_fully_paid: bool,
    pub split_allocated_cents: i64,
    pub split_remaining_cents: i64,
    pub is_split_valid: bool,
    pub new_customer_balance_cents: i64,
    pub credit_paid_now_cents: i64,
    pub credit_remainder_cents: i64,
    /// What the sale request says was received: a credit sale's deposit (always leaving at least
    /// one cent on account), the cash handed over, or the whole total for card, online and split.
    pub amount_received_cents: i64,
}

/// Tendered, change, short by, split allocation and the customer's balance after a credit sale.
pub fn payment_state(input: &PaymentInput) -> AppResult<PaymentState> {
    let too_large = || AppError::validation("That amount is too large");
    let total = input.total_cents;
    let credit_paid_now = if input.is_credit {
        input.credit_deposit_cents.max(0).min(total.max(0))
    } else {
        0
    };
    let credit_remainder = if input.is_credit {
        (total - credit_paid_now).max(0)
    } else {
        0
    };
    let split_allocated = input
        .split_legs_cents
        .iter()
        .try_fold(0_i64, |sum, leg| sum.checked_add(*leg))
        .ok_or_else(too_large)?;
    let split_remaining = if total > 0 {
        (total - split_allocated).max(0)
    } else {
        0
    };
    let is_split_valid = input.method == TenderMethod::Split
        && !input.split_legs_cents.is_empty()
        && split_allocated == total;

    let effective_tendered = if input.is_credit {
        0
    } else {
        match input.method {
            TenderMethod::Cash => input.tendered_cents.max(0),
            TenderMethod::Card | TenderMethod::Online => total,
            TenderMethod::Split => split_allocated,
        }
    };
    Ok(PaymentState {
        effective_tendered_cents: effective_tendered,
        change_due_cents: (effective_tendered - total).max(0),
        short_by_cents: (total - effective_tendered).max(0),
        is_cash_short: input.method == TenderMethod::Cash
            && !input.is_credit
            && effective_tendered < total,
        is_card_short: input.method == TenderMethod::Card
            && !input.is_credit
            && effective_tendered < total,
        is_fully_paid: input.is_credit || effective_tendered >= total,
        split_allocated_cents: split_allocated,
        split_remaining_cents: split_remaining,
        is_split_valid,
        new_customer_balance_cents: input
            .customer_balance_cents
            .checked_add(credit_remainder)
            .ok_or_else(too_large)?,
        credit_paid_now_cents: credit_paid_now,
        credit_remainder_cents: credit_remainder,
        amount_received_cents: if input.is_credit {
            input.credit_deposit_cents.max(0).min((total - 1).max(0))
        } else if input.method == TenderMethod::Cash {
            input.tendered_cents
        } else {
            total
        },
    })
}

/// The cash amounts to offer as quick buttons: the exact total, the next whole rupee, the notes
/// above it, and the next round 500 / 1,000 / 5,000 for larger bills. Sorted, no repeats.
pub fn quick_tender_suggestions(total_cents: i64) -> Vec<i64> {
    use std::collections::BTreeSet;
    if total_cents <= 0 {
        return Vec::new();
    }
    let mut set = BTreeSet::from([total_cents]);
    if total_cents % 100 != 0 {
        set.insert((total_cents / 100 + 1) * 100);
    }
    for note in [2_000, 5_000, 10_000, 50_000, 100_000, 500_000] {
        if note > total_cents {
            set.insert(note);
            if set.len() >= 5 {
                break;
            }
        }
    }
    if total_cents >= 50_000 {
        for step in [50_000, 100_000, 500_000] {
            let next = (total_cents + step - 1) / step * step;
            if next > total_cents {
                set.insert(next);
            }
        }
    }
    set.into_iter().collect()
}

/// What one unit of a returned invoice line is worth back: the line's net total (after its
/// discount) divided by the units sold, to the whole cent below. A line with no units sold falls
/// back to its listed unit price.
pub fn returned_unit_price_cents(
    line_total_cents: i64,
    quantity_sold: i64,
    unit_price_cents: i64,
) -> i64 {
    if quantity_sold > 0 {
        line_total_cents / quantity_sold
    } else {
        unit_price_cents
    }
}

/// Splits `total` across `weights` in proportion, rounding down per leg and giving the last leg
/// whatever the rounding left over, so the legs always add up to exactly `total`. No weights (or
/// none positive) means one leg of the whole.
pub fn proportional_split(total: i64, weights: &[i64]) -> Vec<i64> {
    let weight_sum: i64 = weights.iter().sum();
    if weight_sum <= 0 || weights.is_empty() {
        return vec![total];
    }
    let mut amounts: Vec<i64> = weights
        .iter()
        .map(|w| (i128::from(total) * i128::from(*w) / i128::from(weight_sum)) as i64)
        .collect();
    let allocated: i64 = amounts.iter().sum();
    let remainder = total - allocated;
    if let Some(last) = amounts.last_mut() {
        *last += remainder;
    }
    amounts
}

/// How a net refund is paid out: the cash that goes back, and what only reduces what the customer
/// owes. A positive refund on a real invoice is capped at what was actually paid on it; a no-receipt
/// return has no invoice to cap against and a negative net (the customer pays extra) is never capped.
pub fn refund_amounts(
    net_refund_cents: i64,
    already_paid_cents: i64,
    no_receipt: bool,
) -> (i64, i64) {
    if net_refund_cents > 0 && !no_receipt {
        let cash = net_refund_cents.min(already_paid_cents.max(0));
        (cash, net_refund_cents - cash)
    } else {
        (net_refund_cents, 0)
    }
}

/// Where the refund cash goes by default: in proportion to the invoice's original payment legs when
/// it was paid in more than one, otherwise one leg by the fallback method. Nothing to refund, no legs.
pub fn default_refund_legs(
    refund_cash_cents: i64,
    invoice_legs: &[(String, i64)],
    fallback_method: &str,
) -> Vec<(String, i64)> {
    if refund_cash_cents == 0 {
        return Vec::new();
    }
    let positive: Vec<&(String, i64)> = invoice_legs
        .iter()
        .filter(|(_, amount)| *amount > 0)
        .collect();
    if positive.len() > 1 {
        let weights: Vec<i64> = positive.iter().map(|(_, amount)| *amount).collect();
        positive
            .iter()
            .zip(proportional_split(refund_cash_cents, &weights))
            .map(|((method, _), amount)| (method.clone(), amount))
            .collect()
    } else {
        vec![(fallback_method.to_string(), refund_cash_cents)]
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_share_is_rounded_half_up_and_never_passes_the_whole() {
        assert_eq!(share_basis_points(1, 3), 3_333);
        assert_eq!(share_basis_points(2, 3), 6_667);
        assert_eq!(share_basis_points(1, 8), 1_250);
        assert_eq!(share_basis_points(5, 2), 10_000);
        assert_eq!(share_basis_points(0, 9), 0);
        assert_eq!(share_basis_points(-4, 9), 0);
        assert_eq!(share_basis_points(4, 0), 0);
    }

    use super::*;

    #[test]
    fn test_compute_line_total() {
        assert_eq!(compute_line_total(490, 2, 0), 980);
        assert_eq!(compute_line_total(650, 1, 65), 585);
        assert_eq!(compute_line_total(500, 1, 800), 0);
    }

    #[test]
    fn test_compute_order_discount_percentage_half_up() {
        let adj = PricingAdjustments {
            discount_type: "percentage".to_string(),
            discount_value: 10.0,
        };
        // 1140 * 0.10 = 114.0 -> 114
        let (dtype, dval, dcents) = compute_order_discount(1140, Some(&adj));
        assert_eq!(dtype, "percentage");
        assert_eq!(dval, 10.0);
        assert_eq!(dcents, 114);
    }

    #[test]
    fn test_compute_order_discount_fixed() {
        let adj = PricingAdjustments {
            discount_type: "fixed".to_string(),
            discount_value: 140.0,
        };
        let (_, _, dcents) = compute_order_discount(1140, Some(&adj));
        assert_eq!(dcents, 140);
    }

    #[test]
    fn test_compute_sale_totals() {
        let items = vec![
            InvoiceItem {
                product_key: None,
                name: "Item 1".to_string(),
                sku: None,
                unit_price_cents: 490,
                quantity: 1,
                discount_cents: 0,
                total_cents: 490,
                unit_cost_cents: None,
                source_type: "retail".to_string(),
                source_ticket_key: None,
                source_ticket_number: None,
                assigned_employee_name: None,
                returned_quantity: 0,
                serial_numbers: vec![],
                batch_allocations: vec![],
            },
            InvoiceItem {
                product_key: None,
                name: "Item 2".to_string(),
                sku: None,
                unit_price_cents: 650,
                quantity: 1,
                discount_cents: 0,
                total_cents: 650,
                unit_cost_cents: None,
                source_type: "retail".to_string(),
                source_ticket_key: None,
                source_ticket_number: None,
                assigned_employee_name: None,
                returned_quantity: 0,
                serial_numbers: vec![],
                batch_allocations: vec![],
            },
        ];

        let (subtotal, _, _, discount, total) = compute_sale_totals(&items, None);
        assert_eq!(subtotal, 1140);
        assert_eq!(discount, 0);
        assert_eq!(total, 1140);
    }

    #[test]
    fn test_compute_change_due() {
        assert_eq!(compute_change_due(Some(2000), 1140), Some(860));
        assert_eq!(compute_change_due(Some(1000), 1140), Some(0));
        assert_eq!(compute_change_due(None, 1140), None);
    }

    // ---- Ported from the frontend's posCalculations vectors (now the only copy). ----

    fn retail(unit: i64, quantity: i64, discount: i64) -> CartLine {
        CartLine {
            unit_price_cents: unit,
            quantity,
            discount: LineDiscount::Fixed(discount),
            source: LineSource::Retail,
        }
    }

    #[test]
    fn a_line_has_gross_discount_and_total() {
        let l = line_totals(490, 2, 0).unwrap();
        assert_eq!(
            (l.gross_cents, l.discount_cents, l.total_cents),
            (980, 0, 980)
        );
        let l = line_totals(1000, 1, 150).unwrap();
        assert_eq!(
            (l.gross_cents, l.discount_cents, l.total_cents),
            (1000, 150, 850)
        );
        // The discount never takes a line below zero.
        let l = line_totals(500, 1, 800).unwrap();
        assert_eq!((l.discount_cents, l.total_cents), (500, 0));
        // Negative price, quantity or discount count as none.
        assert_eq!(line_totals(-5, 3, -9).unwrap().total_cents, 0);
        assert!(line_totals(i64::MAX, 2, 0).is_err());
    }

    #[test]
    fn the_order_discount_is_a_percentage_or_fixed_cents_capped_at_the_subtotal() {
        assert_eq!(order_discount_cents(1140, "percentage", 10.0), 114);
        assert_eq!(order_discount_cents(1140, "fixed", 140.0), 140);
        assert_eq!(order_discount_cents(1140, "fixed", 5000.0), 1140);
        assert_eq!(order_discount_cents(1140, "percentage", 250.0), 1140);
        assert_eq!(order_discount_cents(1140, "percentage", 7.5), 86); // 85.5 rounds up
        assert_eq!(order_discount_cents(0, "percentage", 10.0), 0);
        assert_eq!(order_discount_cents(1140, "percentage", -5.0), 0);
        assert_eq!(order_discount_cents(1140, "mystery", 5.0), 0);
    }

    #[test]
    fn percentages_match_the_old_floating_point_answer_on_every_realistic_input() {
        // The previous maths was round(subtotal * pct / 100) in f64. Whole-number maths must give
        // the same cents for every subtotal up to Rs 500 and every percent in steps of 0.25.
        for subtotal in 1..=50_000_i64 {
            for quarter in 0..=400 {
                let pct = f64::from(quarter) / 4.0;
                let old = (((subtotal as f64) * pct / 100.0).round() as i64).clamp(0, subtotal);
                assert_eq!(
                    percentage_discount_cents(subtotal, pct),
                    old,
                    "{subtotal} at {pct}%"
                );
            }
        }
    }

    #[test]
    fn a_returned_unit_is_worth_the_net_line_total_over_the_units_sold() {
        // 1000 net for 3 units: 333 each (to the cent below), the same as the server refunds.
        assert_eq!(returned_unit_price_cents(1000, 3, 400), 333);
        assert_eq!(returned_unit_price_cents(900, 3, 400), 300);
        assert_eq!(returned_unit_price_cents(0, 0, 400), 400);
    }

    #[test]
    fn a_split_adds_up_to_the_total_with_the_remainder_on_the_last_leg() {
        assert_eq!(proportional_split(100, &[1, 1, 1]), vec![33, 33, 34]);
        assert_eq!(proportional_split(1000, &[300, 700]), vec![300, 700]);
        assert_eq!(proportional_split(55, &[]), vec![55]);
        assert_eq!(proportional_split(55, &[0, 0]), vec![55]);
    }

    #[test]
    fn a_refund_is_capped_at_what_was_paid_except_for_no_receipt_and_extra_payments() {
        assert_eq!(refund_amounts(500, 300, false), (300, 200));
        assert_eq!(refund_amounts(500, 900, false), (500, 0));
        assert_eq!(refund_amounts(500, 0, false), (0, 500));
        assert_eq!(refund_amounts(500, 0, true), (500, 0));
        assert_eq!(refund_amounts(-400, 0, false), (-400, 0));
    }

    #[test]
    fn refund_legs_follow_the_invoice_payments_when_there_are_several() {
        let legs = vec![
            ("cash".to_string(), 600),
            ("card".to_string(), 400),
            ("cash".to_string(), -50),
        ];
        assert_eq!(
            default_refund_legs(100, &legs, "cash"),
            vec![("cash".to_string(), 60), ("card".to_string(), 40)]
        );
        let single = vec![("card".to_string(), 900)];
        assert_eq!(
            default_refund_legs(100, &single, "card"),
            vec![("card".to_string(), 100)]
        );
        assert!(default_refund_legs(0, &legs, "cash").is_empty());
    }

    #[test]
    fn a_percentage_line_discount_follows_the_quantity() {
        let line = |quantity| CartLine {
            discount: LineDiscount::Percentage(10.0),
            ..retail(650, quantity, 0)
        };
        let one = cart_totals(&[line(1)], None, 0.0).unwrap();
        assert_eq!(
            (one.lines[0].discount_cents, one.lines[0].total_cents),
            (65, 585)
        );
        let three = cart_totals(&[line(3)], None, 0.0).unwrap();
        assert_eq!(
            (three.lines[0].discount_cents, three.lines[0].total_cents),
            (195, 1755)
        );
        // 15% of 650 is 97.5, which rounds up.
        let half = CartLine {
            discount: LineDiscount::Percentage(15.0),
            ..retail(650, 1, 0)
        };
        assert_eq!(
            cart_totals(&[half], None, 0.0).unwrap().lines[0].discount_cents,
            98
        );
    }

    #[test]
    fn a_cart_adds_up_without_rounding_drift_and_splits_by_source() {
        let t = cart_totals(&[retail(490, 1, 0), retail(650, 1, 0)], None, 0.0).unwrap();
        assert_eq!(
            (t.subtotal_cents, t.order_discount_cents, t.total_cents),
            (1140, 0, 1140)
        );
        assert_eq!((t.lines.len(), t.unit_count, t.retail_cents), (2, 2, 1140));

        let mixed = [
            retail(1000, 1, 0),
            CartLine {
                source: LineSource::Repair,
                ..retail(2500, 1, 0)
            },
            CartLine {
                source: LineSource::Print,
                ..retail(600, 2, 0)
            },
        ];
        let t = cart_totals(&mixed, Some("fixed"), 700.0).unwrap();
        assert_eq!(t.subtotal_cents, 4700);
        assert_eq!(
            (t.retail_cents, t.repairs_cents, t.print_cents),
            (1000, 2500, 1200)
        );
        assert_eq!((t.order_discount_cents, t.total_cents), (700, 4000));
    }

    fn pay(method: TenderMethod, total: i64, tendered: i64) -> PaymentInput<'static> {
        PaymentInput {
            total_cents: total,
            tendered_cents: tendered,
            method,
            split_legs_cents: &[],
            customer_balance_cents: 0,
            is_credit: false,
            credit_deposit_cents: 0,
        }
    }

    #[test]
    fn cash_gives_change_or_a_shortfall() {
        let s = payment_state(&pay(TenderMethod::Cash, 1140, 2000)).unwrap();
        assert_eq!(
            (
                s.effective_tendered_cents,
                s.change_due_cents,
                s.short_by_cents
            ),
            (2000, 860, 0)
        );
        assert!(!s.is_cash_short && s.is_fully_paid);
        let s = payment_state(&pay(TenderMethod::Cash, 1140, 1000)).unwrap();
        assert_eq!((s.change_due_cents, s.short_by_cents), (0, 140));
        assert!(s.is_cash_short && !s.is_fully_paid);
        assert_eq!(s.amount_received_cents, 1000);
        assert_eq!(
            payment_state(&pay(TenderMethod::Card, 1140, 0))
                .unwrap()
                .amount_received_cents,
            1140
        );
    }

    #[test]
    fn card_and_online_pay_exactly_the_total() {
        for method in [TenderMethod::Card, TenderMethod::Online] {
            let s = payment_state(&pay(method, 1140, 0)).unwrap();
            assert_eq!(
                (
                    s.effective_tendered_cents,
                    s.change_due_cents,
                    s.short_by_cents
                ),
                (1140, 0, 0)
            );
            assert!(s.is_fully_paid);
        }
    }

    #[test]
    fn a_split_tracks_what_is_allocated_and_left() {
        let mut input = pay(TenderMethod::Split, 1140, 0);
        input.split_legs_cents = &[640, 500];
        let s = payment_state(&input).unwrap();
        assert_eq!(
            (s.split_allocated_cents, s.split_remaining_cents),
            (1140, 0)
        );
        assert!(s.is_split_valid && s.is_fully_paid);
        input.split_legs_cents = &[640];
        let s = payment_state(&input).unwrap();
        assert_eq!((s.split_remaining_cents, s.short_by_cents), (500, 500));
        assert!(!s.is_split_valid && !s.is_fully_paid);
    }

    #[test]
    fn a_credit_sale_splits_into_a_deposit_and_a_balance_on_account() {
        let mut input = pay(TenderMethod::Cash, 150_000, 0);
        input.is_credit = true;
        input.customer_balance_cents = 20_000;
        input.credit_deposit_cents = 50_000;
        let s = payment_state(&input).unwrap();
        assert_eq!(
            (s.credit_paid_now_cents, s.credit_remainder_cents),
            (50_000, 100_000)
        );
        assert_eq!(
            (s.new_customer_balance_cents, s.effective_tendered_cents),
            (120_000, 0)
        );
        assert!(s.is_fully_paid);
        input.credit_deposit_cents = 900_000; // more than the total is capped
        let s = payment_state(&input).unwrap();
        assert_eq!(
            (s.credit_paid_now_cents, s.credit_remainder_cents),
            (150_000, 0)
        );
        input.credit_deposit_cents = 0;
        let s = payment_state(&input).unwrap();
        assert_eq!(
            (s.credit_paid_now_cents, s.credit_remainder_cents),
            (0, 150_000)
        );
        assert_eq!(s.new_customer_balance_cents, 170_000);
    }

    #[test]
    fn quick_tender_offers_the_exact_total_the_next_rupee_and_notes() {
        let s = quick_tender_suggestions(1140);
        assert_eq!(s[..3], [1140, 1200, 2000]);
        assert_eq!(quick_tender_suggestions(0), Vec::<i64>::new());
        assert_eq!(quick_tender_suggestions(2000)[0], 2000);
        // Larger bills also get the next round 500 / 1,000 / 5,000.
        let big = quick_tender_suggestions(123_456);
        assert!(big.contains(&123_456) && big.contains(&150_000) && big.contains(&200_000));
        assert!(big.contains(&500_000));
        assert!(big.windows(2).all(|w| w[0] < w[1]), "sorted, no repeats");
    }
}

#[cfg(test)]
mod invoice_balance_tests {
    use super::invoice_paid_and_balance;

    #[test]
    fn a_credit_invoice_owes_what_is_left_after_payments_and_returns() {
        assert_eq!(
            invoice_paid_and_balance(true, 100_000, 30_000, 0),
            (30_000, 70_000)
        );
        assert_eq!(
            invoice_paid_and_balance(true, 100_000, 30_000, 20_000),
            (30_000, 50_000)
        );
        // Paid and returned more than the total: nothing is owed, never negative.
        assert_eq!(
            invoice_paid_and_balance(true, 100_000, 90_000, 40_000),
            (90_000, 0)
        );
    }

    #[test]
    fn a_sale_paid_at_the_till_owes_nothing() {
        assert_eq!(
            invoice_paid_and_balance(false, 100_000, 100_000, 0),
            (100_000, 0)
        );
    }
}
