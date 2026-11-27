//! What a print job charges and costs, worked out from its lines. Each line is one thing, a
//! service fee or a part, with a quantity, the price of one, what the whole line costs the shop,
//! and optionally one discount off the line. The job's price is every line's price less the
//! discounts; its cost is the line costs.

use serde::{Deserialize, Serialize};

use crate::error::{CalcError, CalcResult};
use crate::item_discount::{self, ItemDiscount};

const MAX_CENTS: i64 = 10_000_000_000_000;

/// Most lines one print job can have.
pub const MAX_PRINT_ITEMS: usize = 50;

/// Most units one line can have.
pub const MAX_LINE_QUANTITY: i64 = 1_000_000;

const DISCOUNT_MORE_THAN_LINE: &str = "A discount cannot be more than the line's price.";

/// What a line sells, which decides the total it is added to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PrintLineKind {
    /// Work the shop charges for. It costs the shop only what is recorded for it.
    Service,
    /// A part: from stock, or bought in for the job.
    Part,
}

/// What a line costs the shop, in the form the screen knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "basis", rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum CalcPrintCost {
    /// The cost of one unit, as typed for a service or a part bought in.
    PerUnit { cents: i64 },
    /// The cost of the whole line, as the server saved it for a part from stock.
    Line { cents: i64 },
}

/// One line still being typed. A field not typed yet is absent and counts as nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcPrintItem {
    pub kind: PrintLineKind,
    pub quantity: Option<i64>,
    /// What the customer pays for one unit.
    pub unit_price_cents: Option<i64>,
    /// Absent for a service with no recorded cost, which counts as nothing, and for a part
    /// whose cost is not known yet.
    pub cost: Option<CalcPrintCost>,
    /// The discount typed on this line; absent for none.
    pub discount: Option<ItemDiscount>,
}

/// A print job's lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcPrintJobRequest {
    /// The job's lines, in order.
    pub items: Vec<CalcPrintItem>,
}

/// What a print job charges and costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct PrintJobTotals {
    /// All the service lines, before discounts.
    pub services_cents: i64,
    /// All the part lines, before discounts.
    pub parts_cents: i64,
    /// All the line discounts together.
    pub discount_cents: i64,
    /// What the customer pays: services and parts, less the discounts.
    pub price_cents: i64,
    /// What the lines cost the shop.
    pub cost_cents: i64,
}

/// One line worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcPrintLine {
    /// Quantity times the price of one.
    pub gross_cents: i64,
    pub discount_cents: i64,
    /// What the customer pays for the line.
    pub net_cents: i64,
    /// What the line costs the shop; absent while a part's cost is not known.
    pub cost_cents: Option<i64>,
    /// The net less the cost; absent while the cost is not known.
    pub profit_cents: Option<i64>,
}

/// Why a line cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum PrintBlockerCode {
    /// The discount is more than the line's price (or the line has no price yet).
    DiscountMoreThanLine,
}

/// One thing stopping the job being saved, and the line it is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct PrintBlocker {
    /// Position of the line in the request.
    pub item_index: u32,
    pub code: PrintBlockerCode,
}

/// The print form's live answer: the totals, each line worked out, and what stops Save. A line
/// with a blocker counts as having no discount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct PrintJobPreview {
    pub totals: PrintJobTotals,
    /// In the request's order.
    pub lines: Vec<CalcPrintLine>,
    pub blockers: Vec<PrintBlocker>,
    /// False while a part has no cost yet, so the job's profit cannot be shown.
    pub cost_known: bool,
}

/// One line's money once every field is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintItemMoney {
    pub kind: PrintLineKind,
    pub quantity: i64,
    /// What the customer pays for one unit.
    pub unit_price_cents: i64,
    /// What the whole line costs the shop.
    pub cost_cents: i64,
    /// The discount typed on the line.
    pub discount: Option<ItemDiscount>,
}

fn amount_error() -> CalcError {
    CalcError::validation("A print job amount cannot be negative or that large.")
}

fn quantity_error() -> CalcError {
    CalcError::validation(format!(
        "A quantity must be between 1 and {MAX_LINE_QUANTITY}."
    ))
}

/// An amount between zero and the most the system carries.
fn in_range(cents: i64) -> CalcResult<i64> {
    (0..=MAX_CENTS)
        .contains(&cents)
        .then_some(cents)
        .ok_or_else(amount_error)
}

/// `quantity` units at `unit_cents` each.
fn line_amount(quantity: i64, unit_cents: i64) -> CalcResult<i64> {
    let total = i128::from(quantity) * i128::from(in_range(unit_cents)?);
    i64::try_from(total)
        .map_err(|_| amount_error())
        .and_then(in_range)
}

fn too_many_items(count: usize) -> CalcResult<()> {
    if count > MAX_PRINT_ITEMS {
        return Err(CalcError::validation(format!(
            "A print job can have at most {MAX_PRINT_ITEMS} items."
        )));
    }
    Ok(())
}

/// What a line's discount comes to (0 when it has none), or why it is refused.
fn discount_on(
    gross_cents: i64,
    discount: Option<ItemDiscount>,
) -> CalcResult<Result<i64, PrintBlockerCode>> {
    let Some(discount) = discount else {
        return Ok(Ok(0));
    };
    if !discount.is_in_range() {
        return Err(CalcError::validation(item_discount::OUT_OF_RANGE_MESSAGE));
    }
    Ok(discount
        .amount_off(gross_cents)
        .ok_or(PrintBlockerCode::DiscountMoreThanLine))
}

/// What one line's discount comes to in cents (0 when it has none).
///
/// # Errors
///
/// A quantity or amount out of range, or a discount that is out of range or more than the line.
pub fn line_discount_cents(item: &PrintItemMoney) -> CalcResult<i64> {
    if !(1..=MAX_LINE_QUANTITY).contains(&item.quantity) {
        return Err(quantity_error());
    }
    let gross = line_amount(item.quantity, item.unit_price_cents)?;
    discount_on(gross, item.discount)?.map_err(|_| CalcError::validation(DISCOUNT_MORE_THAN_LINE))
}

/// The running sums of a job, wide enough that no list of lines can overflow them.
#[derive(Debug, Default)]
struct Sums {
    services: i128,
    parts: i128,
    discount: i128,
    cost: i128,
}

impl Sums {
    fn add(&mut self, kind: PrintLineKind, gross: i64, discount: i64, cost: i64) {
        match kind {
            PrintLineKind::Service => self.services += i128::from(gross),
            PrintLineKind::Part => self.parts += i128::from(gross),
        }
        self.discount += i128::from(discount);
        self.cost += i128::from(cost);
    }

    fn totals(&self) -> CalcResult<PrintJobTotals> {
        let fit = |total: i128| {
            i64::try_from(total)
                .ok()
                .filter(|cents| *cents <= MAX_CENTS)
                .ok_or_else(|| CalcError::validation("The print job total is too large."))
        };
        Ok(PrintJobTotals {
            services_cents: fit(self.services)?,
            parts_cents: fit(self.parts)?,
            discount_cents: fit(self.discount)?,
            price_cents: fit(self.services + self.parts - self.discount)?,
            cost_cents: fit(self.cost)?,
        })
    }
}

/// Adds up a job's lines.
///
/// # Errors
///
/// Fails when there are too many lines, a quantity or amount is out of range, a discount is
/// refused (see [`line_discount_cents`]), or a total does not fit.
pub fn print_job_totals(items: &[PrintItemMoney]) -> CalcResult<PrintJobTotals> {
    too_many_items(items.len())?;
    let mut sums = Sums::default();
    for item in items {
        let discount = line_discount_cents(item)?;
        let gross = line_amount(item.quantity, item.unit_price_cents)?;
        sums.add(item.kind, gross, discount, in_range(item.cost_cents)?);
    }
    sums.totals()
}

/// Works out the totals of a job still being typed; a missing quantity or amount counts as
/// nothing. A line whose discount is more than its price gets a blocker and counts as having
/// none.
///
/// # Errors
///
/// Fails like [`print_job_totals`], except that a discount that is more than its line is a
/// blocker in the reply, not an error.
pub fn compute(request: &CalcPrintJobRequest) -> CalcResult<PrintJobPreview> {
    too_many_items(request.items.len())?;
    let mut sums = Sums::default();
    let mut lines = Vec::with_capacity(request.items.len());
    let mut blockers = Vec::new();
    let mut cost_known = true;
    for (index, typed) in request.items.iter().enumerate() {
        let quantity = match typed.quantity {
            Some(quantity) if (1..=MAX_LINE_QUANTITY).contains(&quantity) => quantity,
            Some(_) => return Err(quantity_error()),
            None => 0,
        };
        let gross = match typed.unit_price_cents {
            Some(unit) => line_amount(quantity, unit)?,
            None => 0,
        };
        let discount = match discount_on(gross, typed.discount)? {
            Ok(cents) => cents,
            Err(code) => {
                let item_index = u32::try_from(index).expect("a job has at most 50 lines");
                blockers.push(PrintBlocker { item_index, code });
                0
            }
        };
        let cost = match (typed.cost, typed.kind) {
            (Some(CalcPrintCost::PerUnit { cents }), _) => Some(line_amount(quantity, cents)?),
            (Some(CalcPrintCost::Line { cents }), _) => Some(in_range(cents)?),
            (None, PrintLineKind::Service) => Some(0),
            (None, PrintLineKind::Part) => None,
        };
        let net = gross - discount;
        match cost {
            Some(cents) => sums.add(typed.kind, gross, discount, cents),
            None => {
                cost_known = false;
                sums.add(typed.kind, gross, discount, 0);
            }
        }
        lines.push(CalcPrintLine {
            gross_cents: gross,
            discount_cents: discount,
            net_cents: net,
            cost_cents: cost,
            profit_cents: cost.map(|cents| net - cents),
        });
    }
    Ok(PrintJobPreview {
        totals: sums.totals()?,
        lines,
        blockers,
        cost_known,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(quantity: i64, unit_price_cents: i64) -> PrintItemMoney {
        PrintItemMoney {
            kind: PrintLineKind::Service,
            quantity,
            unit_price_cents,
            cost_cents: 0,
            discount: None,
        }
    }

    fn part(quantity: i64, unit_price_cents: i64, cost_cents: i64) -> PrintItemMoney {
        PrintItemMoney {
            kind: PrintLineKind::Part,
            cost_cents,
            ..service(quantity, unit_price_cents)
        }
    }

    fn percent(basis_points: u32) -> ItemDiscount {
        ItemDiscount::Percentage { basis_points }
    }

    fn typed(kind: PrintLineKind) -> CalcPrintItem {
        CalcPrintItem {
            kind,
            quantity: None,
            unit_price_cents: None,
            cost: None,
            discount: None,
        }
    }

    #[test]
    fn multiplies_each_line_by_its_quantity() {
        let totals = print_job_totals(&[service(100, 15_000), part(100, 45_000, 3_000_000)])
            .expect("in range");
        assert_eq!(
            totals,
            PrintJobTotals {
                services_cents: 1_500_000,
                parts_cents: 4_500_000,
                discount_cents: 0,
                price_cents: 6_000_000,
                cost_cents: 3_000_000,
            }
        );
    }

    #[test]
    fn a_discount_comes_off_the_whole_line_and_rounds_half_up() {
        // 12.5% of 3 x 0.35 = 1.05 is 13.125 cents -> 13.
        let line = PrintItemMoney {
            discount: Some(percent(1_250)),
            ..service(3, 35)
        };
        assert_eq!(line_discount_cents(&line), Ok(13));
        let fixed = PrintItemMoney {
            discount: Some(ItemDiscount::Fixed { cents: 105 }),
            ..service(3, 35)
        };
        let totals = print_job_totals(&[fixed]).expect("in range");
        assert_eq!(totals.price_cents, 0);
        assert_eq!(totals.discount_cents, 105);
    }

    #[test]
    fn no_lines_is_a_free_job() {
        let totals = print_job_totals(&[]).expect("in range");
        assert_eq!(totals.price_cents, 0);
        assert_eq!(totals.cost_cents, 0);
    }

    #[test]
    fn refuses_quantities_amounts_and_lists_out_of_range() {
        assert!(print_job_totals(&[service(0, 1)]).is_err());
        assert!(print_job_totals(&[service(MAX_LINE_QUANTITY + 1, 1)]).is_err());
        assert!(print_job_totals(&[service(MAX_LINE_QUANTITY, 1)]).is_ok());
        assert!(print_job_totals(&[service(1, -1)]).is_err());
        assert!(print_job_totals(&[part(1, 1, -1)]).is_err());
        assert!(print_job_totals(&[service(2, MAX_CENTS)]).is_err());
        assert!(print_job_totals(&[service(1, MAX_CENTS), service(1, 1)]).is_err());
        let many = vec![service(1, 1); MAX_PRINT_ITEMS + 1];
        assert!(print_job_totals(&many).is_err());
    }

    #[test]
    fn the_server_refuses_a_discount_that_is_more_than_its_line() {
        let line = PrintItemMoney {
            discount: Some(ItemDiscount::Fixed { cents: 101 }),
            ..service(2, 50)
        };
        assert!(print_job_totals(&[line]).is_err());
        let out_of_range = PrintItemMoney {
            discount: Some(percent(10_001)),
            ..service(1, 100)
        };
        assert!(line_discount_cents(&out_of_range).is_err());
    }

    #[test]
    fn the_preview_works_out_each_line_and_its_profit() {
        let preview = compute(&CalcPrintJobRequest {
            items: vec![
                CalcPrintItem {
                    quantity: Some(10),
                    unit_price_cents: Some(500),
                    cost: Some(CalcPrintCost::PerUnit { cents: 200 }),
                    discount: Some(percent(1_000)),
                    ..typed(PrintLineKind::Part)
                },
                CalcPrintItem {
                    quantity: Some(1),
                    unit_price_cents: Some(1_000),
                    ..typed(PrintLineKind::Service)
                },
                CalcPrintItem {
                    quantity: Some(5),
                    unit_price_cents: Some(100),
                    cost: Some(CalcPrintCost::Line { cents: 700 }),
                    ..typed(PrintLineKind::Part)
                },
            ],
        })
        .expect("in range");
        assert_eq!(preview.lines[0].gross_cents, 5_000);
        assert_eq!(preview.lines[0].discount_cents, 500);
        assert_eq!(preview.lines[0].cost_cents, Some(2_000));
        assert_eq!(preview.lines[0].profit_cents, Some(2_500));
        assert_eq!(preview.lines[1].profit_cents, Some(1_000));
        // A line sold below cost shows the loss.
        assert_eq!(preview.lines[2].profit_cents, Some(-200));
        assert_eq!(preview.totals.price_cents, 4_500 + 1_000 + 500);
        assert_eq!(preview.totals.cost_cents, 2_700);
        assert!(preview.cost_known);
        assert!(preview.blockers.is_empty());
    }

    #[test]
    fn a_service_cost_is_per_unit_and_counts_on_a_line_charged_nothing() {
        let preview = compute(&CalcPrintJobRequest {
            items: vec![
                CalcPrintItem {
                    quantity: Some(3),
                    unit_price_cents: Some(1_000),
                    cost: Some(CalcPrintCost::PerUnit { cents: 400 }),
                    ..typed(PrintLineKind::Service)
                },
                // A free rework line still cost the shop.
                CalcPrintItem {
                    quantity: Some(2),
                    unit_price_cents: Some(0),
                    cost: Some(CalcPrintCost::PerUnit { cents: 250 }),
                    ..typed(PrintLineKind::Service)
                },
            ],
        })
        .expect("in range");
        assert_eq!(preview.lines[0].cost_cents, Some(1_200));
        assert_eq!(preview.lines[0].profit_cents, Some(1_800));
        assert_eq!(preview.lines[1].profit_cents, Some(-500));
        assert_eq!(preview.totals.cost_cents, 1_700);
        assert!(preview.cost_known);
    }

    #[test]
    fn a_part_with_no_cost_yet_hides_the_profit() {
        let preview = compute(&CalcPrintJobRequest {
            items: vec![CalcPrintItem {
                quantity: Some(2),
                unit_price_cents: Some(100),
                ..typed(PrintLineKind::Part)
            }],
        })
        .expect("in range");
        assert!(!preview.cost_known);
        assert_eq!(preview.lines[0].cost_cents, None);
        assert_eq!(preview.lines[0].profit_cents, None);
        assert_eq!(preview.totals.price_cents, 200);
    }

    #[test]
    fn a_half_typed_line_counts_as_nothing_and_a_discount_on_it_is_blocked() {
        let preview = compute(&CalcPrintJobRequest {
            items: vec![CalcPrintItem {
                discount: Some(percent(500)),
                ..typed(PrintLineKind::Service)
            }],
        })
        .expect("in range");
        assert_eq!(preview.totals.price_cents, 0);
        assert_eq!(
            preview.blockers,
            vec![PrintBlocker {
                item_index: 0,
                code: PrintBlockerCode::DiscountMoreThanLine
            }]
        );
    }

    #[test]
    fn the_preview_refuses_a_typed_quantity_of_zero() {
        let request = CalcPrintJobRequest {
            items: vec![CalcPrintItem {
                quantity: Some(0),
                ..typed(PrintLineKind::Service)
            }],
        };
        assert!(compute(&request).is_err());
    }
}
