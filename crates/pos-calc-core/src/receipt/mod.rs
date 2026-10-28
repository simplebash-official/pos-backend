pub mod charges;
pub mod pricing;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::error::{CalcError, CalcResult};
pub use charges::{Allocation, BilledBy, ChargeFigure, ChargeKind, ChargeTotals};
pub use pricing::LineMargin;

pub const CHARGE_DESCRIPTION_MAX: usize = 200;
pub const REFERENCE_MAX: usize = 50;

const MAX_LINES: usize = 200;
const MAX_CHARGES: usize = 50;
const MAX_SERIALS: usize = 20_000;
/// Large enough for any real bill, small enough that sums of 200 lines cannot overflow.
const MAX_CENTS: i64 = 10_000_000_000_000;
const MAX_QUANTITY: i64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PaymentSource {
    Drawer,
    Bank,
    Card,
    Online,
    OwnerPocket,
}

impl PaymentSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentSource::Drawer => "drawer",
            PaymentSource::Bank => "bank",
            PaymentSource::Card => "card",
            PaymentSource::Online => "online",
            PaymentSource::OwnerPocket => "owner_pocket",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(PaymentSource::Drawer),
            "bank" => Some(PaymentSource::Bank),
            "card" => Some(PaymentSource::Card),
            "online" => Some(PaymentSource::Online),
            "owner_pocket" => Some(PaymentSource::OwnerPocket),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum CostMode {
    /// `cost_cents` is the price of one unit.
    Unit,
    /// `cost_cents` is the supplier's price for the whole lot.
    Lot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PricingMode {
    /// The selling price is typed.
    Price,
    /// The selling price is worked out from `margin_tenths`, rounded up to `step_cents`.
    Margin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PaymentMode {
    Credit,
    Full,
    Part,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcChargesPayment {
    pub source: Option<PaymentSource>,
    /// `YYYY-MM-DD`, for the bank, a card and online.
    pub paid_on: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcPayment {
    pub mode: PaymentMode,
    pub source: Option<PaymentSource>,
    /// Only used while `mode` is `part`.
    pub part_cents: i64,
    pub paid_on: String,
    pub category_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcMargin {
    pub profit_each_cents: i64,
    pub lot_profit_cents: i64,
    /// Tenths of a percent (167 is 16.7%).
    pub margin_tenths: i64,
    pub below_cost: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcReceiptLineReply {
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

pub type CalcLineReply = CalcReceiptLineReply;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Blocker {
    pub code: BlockerCode,
    pub line: Option<usize>,
    pub charge: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcReceiptReply {
    pub lines: Vec<CalcReceiptLineReply>,
    pub summary: CalcReceiptSummary,
    /// True while an extra charge is half filled in, so its cost is not counted yet.
    pub charges_incomplete: bool,
    /// True when cash leaves the drawer, which is what a manager's approval is for.
    pub involves_cash: bool,
    /// In the order the form should tell the person; empty means Save may go ahead.
    pub blockers: Vec<Blocker>,
}

fn narrow(value: i128) -> CalcResult<i64> {
    i64::try_from(value).map_err(|_| CalcError::validation("That amount is too large"))
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

fn check_bounds(req: &CalcReceiptRequest) -> CalcResult<()> {
    if req.lines.len() > MAX_LINES || req.charges.len() > MAX_CHARGES {
        return Err(CalcError::validation("There are too many lines or charges"));
    }
    let serials: usize = req.lines.iter().map(|l| l.serial_numbers.len()).sum();
    if serials > MAX_SERIALS {
        return Err(CalcError::validation("There are too many serial numbers"));
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
            return Err(CalcError::validation("An amount is out of range"));
        }
    }
    if req
        .lines
        .iter()
        .any(|l| l.quantity.is_some_and(|q| !(0..=MAX_QUANTITY).contains(&q)))
    {
        return Err(CalcError::validation("A quantity is out of range"));
    }
    Ok(())
}

/// The extra charges filled in far enough to count towards cost, and each line's share of them.
fn spread(
    req: &CalcReceiptRequest,
    figures: &[(i64, i64)],
) -> CalcResult<(Vec<ChargeFigure>, Vec<i64>)> {
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

pub fn compute(req: &CalcReceiptRequest) -> CalcResult<CalcReceiptReply> {
    check_bounds(req)?;

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

fn blockers(
    req: &CalcReceiptRequest,
    quantities: &[i64],
    goods: &[i64],
    landed: &[i64],
    replies: &[CalcReceiptLineReply],
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
