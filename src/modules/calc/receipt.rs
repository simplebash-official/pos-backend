// The Receive Stock form's live figures. The client sends the form as typed so far; this returns
// each line's landed cost, selling price and margin, both sides of the unit/lot and price/margin
// toggles, the receipt summary, and everything that currently stops Save. Pure: no I/O. The money
// maths is `purchases::charges` and `purchases::pricing`, the same code the save path uses.
//
// Fields still being typed are `Option`; an amount the form holds as "not entered" is 0. Money in
// the reply that cannot be worked out yet is `None` or 0 as documented per field, never a guess.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    core::error::{AppError, AppResult},
    domain::purchase_receipts::{Allocation, BilledBy, ChargeKind, PaymentSource},
    modules::purchases::{
        charges::{self, ChargeFigure},
        pricing,
        service::receipt::{CHARGE_DESCRIPTION_MAX, REFERENCE_MAX},
    },
};

const MAX_LINES: usize = 200;
const MAX_CHARGES: usize = 50;
const MAX_SERIALS: usize = 20_000;
/// Large enough for any real bill, small enough that sums of 200 lines cannot overflow.
const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_QUANTITY: i64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CostMode {
    /// `cost_cents` is the price of one unit.
    Unit,
    /// `cost_cents` is the supplier's price for the whole lot.
    Lot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PricingMode {
    /// The selling price is typed.
    Price,
    /// The selling price is worked out from `margin_tenths`, rounded up to `step_cents`.
    Margin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMode {
    Credit,
    Full,
    Part,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcLine {
    pub product_key: Option<String>,
    /// True when the product is tracked by serial number.
    pub serialized: bool,
    /// `None` while the box is empty.
    pub quantity: Option<i64>,
    pub cost_mode: CostMode,
    /// 0 while nothing is typed.
    pub cost_cents: i64,
    pub pricing_mode: PricingMode,
    /// The typed price; only used in `price` mode. 0 while nothing is typed.
    pub selling_price_cents: i64,
    /// Per mille (255 is 25.5%); only used in `margin` mode.
    pub margin_tenths: Option<i64>,
    /// Only used in `margin` mode; 1 means no rounding.
    pub step_cents: Option<i64>,
    pub serial_numbers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCharge {
    pub kind: Option<ChargeKind>,
    /// Always positive; a discount is its own kind. 0 while nothing is typed.
    pub amount_cents: i64,
    pub billed_by: Option<BilledBy>,
    /// Only used with two or more products.
    pub allocation: Option<Allocation>,
    /// Typed text; counted trimmed.
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcChargesPayment {
    pub source: Option<PaymentSource>,
    /// `YYYY-MM-DD`, for the bank, a card and online.
    pub paid_on: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcPayment {
    pub mode: PaymentMode,
    pub source: Option<PaymentSource>,
    /// Only used while `mode` is `part`.
    pub part_cents: i64,
    pub paid_on: String,
    pub category_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcReceiptRequest {
    pub supplier_key: Option<String>,
    pub reference_no: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub lines: Vec<CalcLine>,
    pub charges: Vec<CalcCharge>,
    pub charges_payment: CalcChargesPayment,
    pub payment: CalcPayment,
}

/// What a line earns at its selling price.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcMargin {
    pub profit_each_cents: i64,
    pub lot_profit_cents: i64,
    /// Tenths of a percent (167 is 16.7%).
    pub margin_tenths: i64,
    pub below_cost: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcLineReply {
    /// What the line costs before extra charges.
    pub goods_cents: i64,
    /// Goods plus the line's share of the extra charges that are filled in far enough to count.
    pub landed_cents: i64,
    /// The landed total per unit to the nearest cent, for display; 0 without a quantity.
    pub unit_cost_cents: i64,
    /// The cost as a price each: exact only. `None` when the lot does not divide evenly.
    pub cost_as_unit_cents: Option<i64>,
    /// The cost as the whole-lot price.
    pub cost_as_lot_cents: i64,
    /// The selling price each that will be saved; 0 while it cannot be worked out yet.
    pub selling_price_cents: i64,
    /// In margin mode: the price the margin alone asks for, before rounding up.
    pub price_without_rounding_cents: Option<i64>,
    /// True when rounding up moved the price above what the margin alone asks for.
    pub price_rounded_up: bool,
    pub margin: Option<CalcMargin>,
    /// The margin (tenths) to start from when switching price -> margin; `None` when the price
    /// earns a margin the margin box cannot hold.
    pub margin_tenths_if_switched: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcReceiptSummary {
    pub goods_cents: i64,
    /// On the supplier's bill, discounts taken off.
    pub extra_billed_cents: i64,
    /// What the supplier's bill comes to.
    pub total_cents: i64,
    /// Paid to someone else; not owed to the supplier.
    pub paid_to_others_cents: i64,
    pub paying_now_cents: i64,
    pub left_owed_cents: i64,
    /// All the money leaving today.
    pub cash_out_today_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlockerCode {
    SupplierMissing,
    DateInvalid,
    ReferenceTooLong,
    NoLines,
    ProductMissing,
    ProductDuplicate,
    QuantityMissing,
    CostLotMissing,
    CostUnitMissing,
    MarginMissing,
    StepMissing,
    MarginUnreachable,
    PriceMissing,
    SerialsMissing,
    SerialsDuplicate,
    ChargeKindMissing,
    ChargeAmountMissing,
    ChargeBilledByMissing,
    ChargeDiscountNotOnSupplierBill,
    ChargeDescriptionTooLong,
    ChargeAllocationMissing,
    ChargesSourceMissing,
    ChargesDayInvalid,
    DiscountsExceedLine,
    DiscountsExceedBill,
    PaymentSourceMissing,
    PaymentCategoryMissing,
    PartAmountMissing,
    PartIsWholeBill,
    PaymentDayInvalid,
}

/// One thing that stops the receipt from being saved. `line` / `charge` are zero-based positions.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Blocker {
    pub code: BlockerCode,
    pub line: Option<usize>,
    pub charge: Option<usize>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcReceiptReply {
    pub lines: Vec<CalcLineReply>,
    pub summary: CalcReceiptSummary,
    /// True while an extra charge is half filled in, so its cost is not counted yet.
    pub charges_incomplete: bool,
    /// True when cash leaves the drawer, which is what a manager's approval is for.
    pub involves_cash: bool,
    /// In the order the form should tell the person; empty means Save may go ahead.
    pub blockers: Vec<Blocker>,
}

fn narrow(value: i128) -> AppResult<i64> {
    i64::try_from(value).map_err(|_| AppError::validation("That amount is too large"))
}

fn is_cash(source: Option<PaymentSource>) -> bool {
    matches!(
        source,
        Some(PaymentSource::Drawer | PaymentSource::OwnerPocket)
    )
}

/// A real calendar day written `YYYY-MM-DD`.
fn is_day(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn check_bounds(req: &CalcReceiptRequest) -> AppResult<()> {
    if req.lines.len() > MAX_LINES || req.charges.len() > MAX_CHARGES {
        return Err(AppError::validation("There are too many lines or charges"));
    }
    let serials: usize = req.lines.iter().map(|l| l.serial_numbers.len()).sum();
    if serials > MAX_SERIALS {
        return Err(AppError::validation("There are too many serial numbers"));
    }
    let money = req
        .lines
        .iter()
        .map(|l| l.cost_cents)
        .chain(req.lines.iter().map(|l| l.selling_price_cents))
        .chain(req.charges.iter().map(|c| c.amount_cents))
        .chain([req.payment.part_cents]);
    for cents in money {
        if !(0..=MAX_CENTS).contains(&cents) {
            return Err(AppError::validation("An amount is out of range"));
        }
    }
    if req
        .lines
        .iter()
        .any(|l| l.quantity.is_some_and(|q| !(0..=MAX_QUANTITY).contains(&q)))
    {
        return Err(AppError::validation("A quantity is out of range"));
    }
    Ok(())
}

/// The extra charges filled in far enough to count towards cost, and each line's share of them.
fn spread(
    req: &CalcReceiptRequest,
    figures: &[(i64, i64)],
) -> AppResult<(Vec<ChargeFigure>, Vec<i64>)> {
    let mut shares = vec![0_i64; figures.len()];
    let mut counted = Vec::new();
    for charge in &req.charges {
        let (Some(kind), Some(billed_by)) = (charge.kind, charge.billed_by) else {
            continue;
        };
        if charge.amount_cents <= 0 || figures.is_empty() {
            continue;
        }
        let amount = charges::signed_amount(kind, charge.amount_cents);
        let parts = if figures.len() == 1 {
            vec![amount]
        } else {
            let Some(allocation) = charge.allocation else {
                continue;
            };
            let weights = charges::weights(allocation, figures);
            // A charge cannot be spread over lines that carry nothing yet.
            if weights.iter().map(|&w| i128::from(w)).sum::<i128>() <= 0 {
                continue;
            }
            charges::allocate(amount, &weights)
        };
        for (share, part) in shares.iter_mut().zip(parts) {
            *share = narrow(i128::from(*share) + i128::from(part))?;
        }
        counted.push(ChargeFigure {
            kind,
            amount_cents: charge.amount_cents,
            billed_by,
        });
    }
    Ok((counted, shares))
}

pub fn compute(req: &CalcReceiptRequest) -> AppResult<CalcReceiptReply> {
    check_bounds(req)?;

    // No quantity typed yet counts as none.
    let quantities: Vec<i64> = req.lines.iter().map(|l| l.quantity.unwrap_or(0)).collect();
    let mut goods = Vec::with_capacity(req.lines.len());
    for (line, &quantity) in req.lines.iter().zip(&quantities) {
        goods.push(match line.cost_mode {
            CostMode::Lot => line.cost_cents,
            CostMode::Unit => narrow(i128::from(quantity) * i128::from(line.cost_cents))?,
        });
    }
    let figures: Vec<(i64, i64)> = quantities
        .iter()
        .copied()
        .zip(goods.iter().copied())
        .collect();
    let (counted, shares) = spread(req, &figures)?;
    let charges_incomplete = counted.len() != req.charges.len();

    let mut lines = Vec::with_capacity(req.lines.len());
    let mut landed_all = Vec::with_capacity(req.lines.len());
    for (index, line) in req.lines.iter().enumerate() {
        let (quantity, goods_cents) = figures[index];
        let landed = narrow(i128::from(goods_cents) + i128::from(shares[index]))?;
        landed_all.push(landed);

        let selling = match line.pricing_mode {
            PricingMode::Price => line.selling_price_cents,
            PricingMode::Margin => match (line.margin_tenths, line.step_cents) {
                (Some(tenths), Some(step)) => {
                    pricing::suggested_price(landed, quantity, tenths, step)?.unwrap_or(0)
                }
                _ => 0,
            },
        };
        let price_without_rounding_cents = match (line.pricing_mode, line.margin_tenths) {
            (PricingMode::Margin, Some(tenths)) => {
                pricing::suggested_price(landed, quantity, tenths, 1)?
            }
            _ => None,
        };
        let margin = pricing::line_margin(landed, quantity, selling)?;
        let cost_as_unit_cents = match line.cost_mode {
            CostMode::Unit => Some(line.cost_cents),
            CostMode::Lot if goods_cents == 0 => Some(0),
            CostMode::Lot if quantity < 1 || goods_cents % quantity != 0 => None,
            CostMode::Lot => Some(goods_cents / quantity),
        };
        lines.push(CalcLineReply {
            goods_cents,
            landed_cents: landed,
            unit_cost_cents: charges::rounded_unit(landed, quantity),
            cost_as_unit_cents,
            cost_as_lot_cents: goods_cents,
            selling_price_cents: selling,
            price_rounded_up: price_without_rounding_cents.is_some_and(|exact| selling > exact),
            price_without_rounding_cents,
            margin_tenths_if_switched: margin
                .as_ref()
                .map(|m| m.margin_tenths)
                .filter(|tenths| (0..1000).contains(tenths)),
            margin: margin.map(|m| CalcMargin {
                profit_each_cents: m.profit_each_cents,
                lot_profit_cents: m.lot_profit_cents,
                margin_tenths: m.margin_tenths,
                below_cost: m.below_cost,
            }),
        });
    }

    let goods_total = narrow(goods.iter().map(|&g| i128::from(g)).sum())?;
    let totals = charges::charge_totals(&counted);
    let total_cents = narrow(i128::from(goods_total) + i128::from(totals.supplier_billed_cents))?;
    let paying_now_cents = match req.payment.mode {
        PaymentMode::Credit => 0,
        PaymentMode::Full => total_cents,
        PaymentMode::Part => req.payment.part_cents,
    };
    let summary = CalcReceiptSummary {
        goods_cents: goods_total,
        extra_billed_cents: totals.supplier_billed_cents,
        total_cents,
        paid_to_others_cents: totals.paid_to_others_cents,
        paying_now_cents,
        left_owed_cents: narrow(i128::from(total_cents) - i128::from(paying_now_cents))?,
        cash_out_today_cents: narrow(
            i128::from(paying_now_cents) + i128::from(totals.paid_to_others_cents),
        )?,
    };

    let has_third_party = req
        .charges
        .iter()
        .any(|c| c.billed_by == Some(BilledBy::ThirdParty));
    let involves_cash = (req.payment.mode != PaymentMode::Credit && is_cash(req.payment.source))
        || (has_third_party && is_cash(req.charges_payment.source));

    let blockers = blockers(req, &quantities, &goods, &landed_all, &lines, &summary);
    Ok(CalcReceiptReply {
        lines,
        summary,
        charges_incomplete,
        involves_cash,
        blockers,
    })
}

fn blocker(code: BlockerCode, line: Option<usize>, charge: Option<usize>) -> Blocker {
    Blocker { code, line, charge }
}

/// Everything that stops the receipt from being saved, in the order the form should say it.
fn blockers(
    req: &CalcReceiptRequest,
    quantities: &[i64],
    goods: &[i64],
    landed: &[i64],
    replies: &[CalcLineReply],
    summary: &CalcReceiptSummary,
) -> Vec<Blocker> {
    use BlockerCode as B;
    let mut out = Vec::new();
    if req.supplier_key.as_deref().is_none_or(str::is_empty) {
        out.push(blocker(B::SupplierMissing, None, None));
    }
    if !is_day(&req.date) {
        out.push(blocker(B::DateInvalid, None, None));
    }
    if req.reference_no.trim().chars().count() > REFERENCE_MAX {
        out.push(blocker(B::ReferenceTooLong, None, None));
    }
    if req.lines.is_empty() {
        out.push(blocker(B::NoLines, None, None));
    }

    let mut seen = HashSet::new();
    for (index, line) in req.lines.iter().enumerate() {
        let at = Some(index);
        let quantity = quantities[index];
        let problem = if line.product_key.as_deref().is_none_or(str::is_empty) {
            Some(B::ProductMissing)
        } else if !seen.insert(line.product_key.clone()) {
            Some(B::ProductDuplicate)
        } else if quantity < 1 {
            Some(B::QuantityMissing)
        } else if goods[index] <= 0 {
            Some(match line.cost_mode {
                CostMode::Lot => B::CostLotMissing,
                CostMode::Unit => B::CostUnitMissing,
            })
        } else {
            match line.pricing_mode {
                PricingMode::Margin if line.margin_tenths.is_none() => Some(B::MarginMissing),
                PricingMode::Margin if line.step_cents.is_none() => Some(B::StepMissing),
                PricingMode::Margin if replies[index].selling_price_cents <= 0 => {
                    Some(B::MarginUnreachable)
                }
                PricingMode::Price if line.selling_price_cents <= 0 => Some(B::PriceMissing),
                _ => {
                    let blank = line.serial_numbers.iter().any(|s| s.trim().is_empty());
                    (line.serialized && (line.serial_numbers.len() as i64 != quantity || blank))
                        .then_some(B::SerialsMissing)
                }
            }
        };
        if let Some(code) = problem {
            out.push(blocker(code, at, None));
        }
    }
    let serials: Vec<&str> = req
        .lines
        .iter()
        .flat_map(|l| l.serial_numbers.iter().map(|s| s.trim()))
        .collect();
    if serials.iter().collect::<HashSet<_>>().len() != serials.len() {
        out.push(blocker(B::SerialsDuplicate, None, None));
    }

    for (index, charge) in req.charges.iter().enumerate() {
        let at = Some(index);
        let code = if charge.kind.is_none() {
            Some(B::ChargeKindMissing)
        } else if charge.amount_cents <= 0 {
            Some(B::ChargeAmountMissing)
        } else if charge.billed_by.is_none() {
            Some(B::ChargeBilledByMissing)
        } else if charge.kind == Some(ChargeKind::Discount)
            && charge.billed_by != Some(BilledBy::Supplier)
        {
            Some(B::ChargeDiscountNotOnSupplierBill)
        } else if charge.description.trim().chars().count() > CHARGE_DESCRIPTION_MAX {
            Some(B::ChargeDescriptionTooLong)
        } else if req.lines.len() > 1 && charge.allocation.is_none() {
            Some(B::ChargeAllocationMissing)
        } else {
            None
        };
        if let Some(code) = code {
            out.push(blocker(code, None, at));
        }
    }
    if req
        .charges
        .iter()
        .any(|c| c.billed_by == Some(BilledBy::ThirdParty))
    {
        let paid = &req.charges_payment;
        if paid.source.is_none() {
            out.push(blocker(B::ChargesSourceMissing, None, None));
        } else if !is_cash(paid.source) && !is_day(&paid.paid_on) {
            out.push(blocker(B::ChargesDayInvalid, None, None));
        }
    }
    if landed.iter().any(|&cents| cents <= 0) && !req.lines.is_empty() {
        out.push(blocker(B::DiscountsExceedLine, None, None));
    }
    if summary.total_cents <= 0 {
        out.push(blocker(B::DiscountsExceedBill, None, None));
    }

    let payment = &req.payment;
    if payment.mode != PaymentMode::Credit {
        if payment.source.is_none() {
            out.push(blocker(B::PaymentSourceMissing, None, None));
        } else if payment.category_key.as_deref().is_none_or(str::is_empty) {
            out.push(blocker(B::PaymentCategoryMissing, None, None));
        } else if payment.mode == PaymentMode::Part && payment.part_cents <= 0 {
            out.push(blocker(B::PartAmountMissing, None, None));
        } else if payment.mode == PaymentMode::Part && payment.part_cents >= summary.total_cents {
            out.push(blocker(B::PartIsWholeBill, None, None));
        } else if !is_cash(payment.source) && !is_day(&payment.paid_on) {
            out.push(blocker(B::PaymentDayInvalid, None, None));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(quantity: i64, unit_cents: i64) -> CalcLine {
        CalcLine {
            product_key: Some(format!("prod_{quantity}_{unit_cents}")),
            serialized: false,
            quantity: Some(quantity),
            cost_mode: CostMode::Unit,
            cost_cents: unit_cents,
            pricing_mode: PricingMode::Price,
            selling_price_cents: 1,
            margin_tenths: None,
            step_cents: None,
            serial_numbers: vec![],
        }
    }

    fn charge(kind: ChargeKind, cents: i64, by: BilledBy, how: Option<Allocation>) -> CalcCharge {
        CalcCharge {
            kind: Some(kind),
            amount_cents: cents,
            billed_by: Some(by),
            allocation: how,
            description: String::new(),
        }
    }

    fn request(lines: Vec<CalcLine>, charges: Vec<CalcCharge>) -> CalcReceiptRequest {
        CalcReceiptRequest {
            supplier_key: Some("sup_1".into()),
            reference_no: String::new(),
            date: "2026-10-26".into(),
            lines,
            charges,
            charges_payment: CalcChargesPayment {
                source: Some(PaymentSource::Drawer),
                paid_on: String::new(),
            },
            payment: CalcPayment {
                mode: PaymentMode::Full,
                source: Some(PaymentSource::Bank),
                part_cents: 0,
                paid_on: "2026-10-26".into(),
                category_key: Some("ccat_1".into()),
            },
        }
    }

    #[test]
    fn one_line_with_a_courier_charge_and_a_margin() {
        let mut one = line(10, 5_000);
        one.pricing_mode = PricingMode::Margin;
        one.margin_tenths = Some(250);
        one.step_cents = Some(100);
        let req = request(
            vec![one],
            vec![charge(
                ChargeKind::Shipping,
                3_000,
                BilledBy::ThirdParty,
                None,
            )],
        );
        let reply = compute(&req).unwrap();
        let l = &reply.lines[0];
        assert_eq!(l.goods_cents, 50_000);
        assert_eq!(l.landed_cents, 53_000);
        assert_eq!(l.unit_cost_cents, 5_300);
        assert_eq!(l.selling_price_cents, 7_100);
        assert_eq!(l.price_without_rounding_cents, Some(7_067));
        assert!(l.price_rounded_up);
        let m = l.margin.as_ref().unwrap();
        assert_eq!(
            (m.profit_each_cents, m.lot_profit_cents, m.margin_tenths),
            (1_800, 18_000, 254)
        );
        assert_eq!(l.margin_tenths_if_switched, Some(254));
        assert_eq!(l.cost_as_unit_cents, Some(5_000));
        assert_eq!(l.cost_as_lot_cents, 50_000);
        let s = &reply.summary;
        assert_eq!(
            (
                s.goods_cents,
                s.extra_billed_cents,
                s.total_cents,
                s.paid_to_others_cents
            ),
            (50_000, 0, 50_000, 3_000)
        );
        assert_eq!(
            (
                s.paying_now_cents,
                s.left_owed_cents,
                s.cash_out_today_cents
            ),
            (50_000, 0, 53_000)
        );
        assert!(!reply.charges_incomplete);
        assert!(reply.involves_cash);
        assert!(reply.blockers.is_empty(), "{:?}", reply.blockers);
    }

    #[test]
    fn a_supplier_charge_is_spread_by_value_to_the_cent() {
        let req = request(
            vec![line(1, 100), line(1, 300)],
            vec![charge(
                ChargeKind::Insurance,
                40,
                BilledBy::Supplier,
                Some(Allocation::Value),
            )],
        );
        let reply = compute(&req).unwrap();
        assert_eq!(reply.lines[0].landed_cents, 110);
        assert_eq!(reply.lines[1].landed_cents, 330);
        assert_eq!(reply.summary.extra_billed_cents, 40);
        assert_eq!(reply.summary.total_cents, 440);
    }

    #[test]
    fn a_half_filled_charge_is_not_counted_and_says_so() {
        let mut half = charge(ChargeKind::Shipping, 500, BilledBy::Supplier, None);
        half.billed_by = None;
        let reply = compute(&request(vec![line(2, 1_000)], vec![half])).unwrap();
        assert!(reply.charges_incomplete);
        assert_eq!(reply.lines[0].landed_cents, 2_000);
        assert!(
            reply
                .blockers
                .iter()
                .any(|b| b.code == BlockerCode::ChargeBilledByMissing)
        );
    }

    #[test]
    fn a_value_charge_over_unpriced_lines_waits_instead_of_dividing_by_zero() {
        let req = request(
            vec![line(1, 0), line(1, 0)],
            vec![charge(
                ChargeKind::Shipping,
                500,
                BilledBy::Supplier,
                Some(Allocation::Value),
            )],
        );
        let reply = compute(&req).unwrap();
        assert!(reply.charges_incomplete);
    }

    #[test]
    fn a_lot_that_does_not_divide_has_no_price_each() {
        let mut lot = line(3, 0);
        lot.cost_mode = CostMode::Lot;
        lot.cost_cents = 1_000;
        let reply = compute(&request(vec![lot], vec![])).unwrap();
        assert_eq!(reply.lines[0].cost_as_unit_cents, None);
        assert_eq!(reply.lines[0].unit_cost_cents, 333);
    }

    #[test]
    fn blockers_come_in_the_order_the_form_says_them() {
        let mut req = request(vec![line(0, 0)], vec![]);
        req.supplier_key = None;
        req.date = "2026-02-30".into();
        let codes: Vec<_> = compute(&req)
            .unwrap()
            .blockers
            .iter()
            .map(|b| b.code)
            .collect();
        assert_eq!(
            codes[..3],
            [
                BlockerCode::SupplierMissing,
                BlockerCode::DateInvalid,
                BlockerCode::QuantityMissing
            ]
        );
    }

    #[test]
    fn discounts_bigger_than_the_line_block_saving() {
        let req = request(
            vec![line(1, 100)],
            vec![charge(ChargeKind::Discount, 150, BilledBy::Supplier, None)],
        );
        let codes: Vec<_> = compute(&req)
            .unwrap()
            .blockers
            .iter()
            .map(|b| b.code)
            .collect();
        assert!(codes.contains(&BlockerCode::DiscountsExceedLine));
        assert!(codes.contains(&BlockerCode::DiscountsExceedBill));
    }

    #[test]
    fn out_of_range_input_is_refused() {
        let mut req = request(vec![line(1, 100)], vec![]);
        req.lines[0].cost_cents = -1;
        assert!(compute(&req).is_err());
        req.lines[0].cost_cents = MAX_CENTS;
        req.lines[0].quantity = Some(MAX_QUANTITY);
        assert!(compute(&req).is_err());
    }
}
