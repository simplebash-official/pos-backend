//! What a repair job charges and costs, worked out from its items. Each item is a piece of
//! work: labour with what it cost the shop when that is recorded, or at most one part with the
//! price the customer pays for it and what the part cost the shop, and optionally one discount
//! off that item. The job's price is all the labour and part prices less the discounts; its cost
//! is the part costs and the recorded labour costs.

use serde::{Deserialize, Serialize};

use crate::error::{CalcError, CalcResult};
use crate::item_discount::{self, ItemDiscount};

const MAX_CENTS: i64 = 10_000_000_000_000;

/// Most items one job can have.
pub const MAX_REPAIR_ITEMS: usize = 50;

/// The money on one repair item. A field still being typed is absent and counts as nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcRepairItem {
    /// What the work itself is charged.
    pub labor_cents: Option<i64>,
    /// What the work cost the shop; absent when none is recorded.
    pub labor_cost_cents: Option<i64>,
    /// What the customer pays for the part.
    pub part_cents: Option<i64>,
    /// What the part cost the shop.
    pub part_cost_cents: Option<i64>,
    /// The discount typed on this item; absent for none.
    pub discount: Option<ItemDiscount>,
}

/// A job's items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcRepairRequest {
    /// The job's items, in order.
    pub items: Vec<CalcRepairItem>,
}

/// What a job charges and costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct RepairTotals {
    /// All the labour, before discounts.
    pub labor_cents: i64,
    /// All the part prices, before discounts.
    pub parts_cents: i64,
    /// All the item discounts together.
    pub discount_cents: i64,
    /// What the customer pays: labour and parts, less the discounts.
    pub price_cents: i64,
    /// What the parts and the recorded labour cost the shop.
    pub cost_cents: i64,
}

/// Why an item cannot be saved, in the order the form should say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum RepairBlockerCode {
    /// The discount is more than the item's price (or the item has no price yet).
    DiscountMoreThanItem,
    /// The discount sits on an item that has both a fee and a part.
    DiscountOnMixedItem,
}

/// One thing stopping the job being saved, and the item it is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct RepairBlocker {
    /// Position of the item in the request.
    pub item_index: u32,
    pub code: RepairBlockerCode,
}

/// The repair form's live answer: the totals, each item's discount and profit worked out, and
/// what stops Save. An item with a blocker counts as having no discount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct RepairPreview {
    pub totals: RepairTotals,
    /// What each item's discount comes to, in the request's order; 0 for none.
    pub item_discounts_cents: Vec<i64>,
    /// Each item's price less its discount and what it cost, in the request's order. Negative
    /// when it cost more than it is charged.
    pub item_profits_cents: Vec<i64>,
    pub blockers: Vec<RepairBlocker>,
}

/// One item's money once every field is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairItemMoney {
    /// What the work itself is charged.
    pub labor_cents: i64,
    /// What the work cost the shop; absent when none is recorded.
    pub labor_cost_cents: Option<i64>,
    /// What the customer pays for the part.
    pub part_cents: i64,
    /// What the part cost the shop.
    pub part_cost_cents: i64,
    /// The discount typed on the item.
    pub discount: Option<ItemDiscount>,
}

impl RepairItemMoney {
    /// What the item cost the shop: its part and its recorded labour cost, wide enough that
    /// amounts not yet checked cannot overflow.
    fn cost_cents(&self) -> i128 {
        i128::from(self.part_cost_cents)
            + self
                .labor_cost_cents
                .into_iter()
                .map(i128::from)
                .sum::<i128>()
    }
}

/// Why an item's discount is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscountProblem {
    /// A percentage outside 0.01% to 100%, or a fixed amount under 1 cent.
    OutOfRange,
    /// The item has both a fee and a part, so the discount has no clear home.
    OnMixedItem,
    /// More than the item's price, or the item has no price.
    MoreThanItem,
}

impl DiscountProblem {
    fn message(self) -> &'static str {
        match self {
            DiscountProblem::OutOfRange => item_discount::OUT_OF_RANGE_MESSAGE,
            DiscountProblem::OnMixedItem => {
                "Give the discount to the fee or to the part, not to one item with both."
            }
            DiscountProblem::MoreThanItem => "A discount cannot be more than the item's price.",
        }
    }
}

/// What the item's discount comes to in cents, or why it is refused. No discount is 0.
fn check_discount(item: &RepairItemMoney) -> Result<i64, DiscountProblem> {
    let Some(discount) = item.discount else {
        return Ok(0);
    };
    if !discount.is_in_range() {
        return Err(DiscountProblem::OutOfRange);
    }
    if item.labor_cents > 0 && item.part_cents > 0 {
        return Err(DiscountProblem::OnMixedItem);
    }
    discount
        .amount_off(item.labor_cents + item.part_cents)
        .ok_or(DiscountProblem::MoreThanItem)
}

/// What one item's discount comes to in cents (0 when it has none). The amounts must already be
/// in range (see [`repair_totals`]).
///
/// # Errors
///
/// Fails when the discount is out of range, sits on an item with both a fee and a part, or is
/// more than the item's price.
pub fn item_discount_cents(item: &RepairItemMoney) -> CalcResult<i64> {
    check_discount(item).map_err(|problem| CalcError::validation(problem.message()))
}

/// Adds up a job's items.
///
/// # Errors
///
/// Fails when there are too many items, an amount is negative or too large, a discount is
/// refused (see [`item_discount_cents`]), or a total does not fit.
pub fn repair_totals(items: &[RepairItemMoney]) -> CalcResult<RepairTotals> {
    if items.len() > MAX_REPAIR_ITEMS {
        return Err(CalcError::validation(format!(
            "A repair job can have at most {MAX_REPAIR_ITEMS} items."
        )));
    }
    let in_range = |cents: i64| (0..=MAX_CENTS).contains(&cents);
    let mut labor: i128 = 0;
    let mut parts: i128 = 0;
    let mut cost: i128 = 0;
    let mut discount: i128 = 0;
    for item in items {
        if !in_range(item.labor_cents)
            || !item.labor_cost_cents.into_iter().all(in_range)
            || !in_range(item.part_cents)
            || !in_range(item.part_cost_cents)
        {
            return Err(CalcError::validation(
                "A repair amount cannot be negative or that large.",
            ));
        }
        labor += i128::from(item.labor_cents);
        parts += i128::from(item.part_cents);
        cost += item.cost_cents();
        discount += i128::from(item_discount_cents(item)?);
    }
    let fit = |total: i128| {
        i64::try_from(total)
            .ok()
            .filter(|cents| *cents <= MAX_CENTS)
            .ok_or_else(|| CalcError::validation("The repair total is too large."))
    };
    Ok(RepairTotals {
        labor_cents: fit(labor)?,
        parts_cents: fit(parts)?,
        discount_cents: fit(discount)?,
        price_cents: fit(labor + parts - discount)?,
        cost_cents: fit(cost)?,
    })
}

/// Works out the totals of a job still being typed; a missing amount counts as nothing. An item
/// whose discount cannot stand gets a blocker and counts as having none.
///
/// # Errors
///
/// Fails like [`repair_totals`], except that a discount that is more than its item or sits on
/// a mixed item is a blocker in the reply, not an error.
pub fn compute(request: &CalcRepairRequest) -> CalcResult<RepairPreview> {
    let mut items = Vec::with_capacity(request.items.len());
    let mut item_discounts_cents = Vec::with_capacity(request.items.len());
    let mut blockers = Vec::new();
    for (index, typed) in request.items.iter().enumerate() {
        let mut money = RepairItemMoney {
            labor_cents: typed.labor_cents.unwrap_or(0),
            labor_cost_cents: typed.labor_cost_cents,
            part_cents: typed.part_cents.unwrap_or(0),
            part_cost_cents: typed.part_cost_cents.unwrap_or(0),
            discount: typed.discount,
        };
        let code = match check_discount(&money) {
            Ok(cents) => {
                item_discounts_cents.push(cents);
                None
            }
            Err(DiscountProblem::OutOfRange) => {
                return Err(CalcError::validation(DiscountProblem::OutOfRange.message()));
            }
            Err(DiscountProblem::OnMixedItem) => Some(RepairBlockerCode::DiscountOnMixedItem),
            Err(DiscountProblem::MoreThanItem) => Some(RepairBlockerCode::DiscountMoreThanItem),
        };
        if let Some(code) = code {
            item_discounts_cents.push(0);
            money.discount = None;
            let item_index = u32::try_from(index).expect("a job has at most 50 items");
            blockers.push(RepairBlocker { item_index, code });
        }
        items.push(money);
    }
    let totals = repair_totals(&items)?;
    // The totals above refused any amount out of range, so each profit fits.
    let item_profits_cents = items
        .iter()
        .zip(&item_discounts_cents)
        .map(|(item, discount_cents)| {
            let profit = i128::from(item.labor_cents) + i128::from(item.part_cents)
                - i128::from(*discount_cents)
                - item.cost_cents();
            i64::try_from(profit).expect("an item's amounts are in range")
        })
        .collect();
    Ok(RepairPreview {
        totals,
        item_discounts_cents,
        item_profits_cents,
        blockers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(labor: i64, part: i64, cost: i64) -> RepairItemMoney {
        RepairItemMoney {
            labor_cents: labor,
            labor_cost_cents: None,
            part_cents: part,
            part_cost_cents: cost,
            discount: None,
        }
    }

    fn costed_fee(labor: i64, labor_cost: i64) -> RepairItemMoney {
        RepairItemMoney {
            labor_cost_cents: Some(labor_cost),
            ..item(labor, 0, 0)
        }
    }

    fn discounted(labor: i64, part: i64, discount: ItemDiscount) -> RepairItemMoney {
        RepairItemMoney {
            discount: Some(discount),
            ..item(labor, part, 0)
        }
    }

    fn percent(basis_points: u32) -> ItemDiscount {
        ItemDiscount::Percentage { basis_points }
    }

    fn typed(
        labor: Option<i64>,
        part: Option<i64>,
        discount: Option<ItemDiscount>,
    ) -> CalcRepairItem {
        CalcRepairItem {
            labor_cents: labor,
            labor_cost_cents: None,
            part_cents: part,
            part_cost_cents: None,
            discount,
        }
    }

    #[test]
    fn a_labour_cost_adds_to_the_job_cost_but_not_to_the_price() {
        let totals = repair_totals(&[costed_fee(250_000, 100_000), item(0, 1_200_000, 850_000)])
            .expect("in range");
        assert_eq!(totals.price_cents, 1_450_000);
        assert_eq!(totals.cost_cents, 950_000);
    }

    #[test]
    fn refuses_a_labour_cost_out_of_range() {
        assert!(repair_totals(&[costed_fee(1, -1)]).is_err());
        assert!(repair_totals(&[costed_fee(1, MAX_CENTS + 1)]).is_err());
        assert!(repair_totals(&[costed_fee(1, MAX_CENTS), item(0, 0, 1)]).is_err());
    }

    #[test]
    fn the_preview_refuses_an_amount_too_large_to_add_up() {
        let request = CalcRepairRequest {
            items: vec![CalcRepairItem {
                labor_cost_cents: Some(i64::MAX),
                part_cost_cents: Some(i64::MAX),
                ..typed(None, None, None)
            }],
        };
        assert!(compute(&request).is_err());
    }

    #[test]
    fn the_preview_gives_each_item_its_profit_after_discount_and_cost() {
        let preview = compute(&CalcRepairRequest {
            items: vec![
                CalcRepairItem {
                    labor_cost_cents: Some(100_000),
                    ..typed(Some(250_000), None, Some(percent(1_000)))
                },
                // Sold below cost: the loss is shown.
                CalcRepairItem {
                    part_cost_cents: Some(900),
                    ..typed(None, Some(500), None)
                },
                // A free rework still cost the shop.
                CalcRepairItem {
                    labor_cost_cents: Some(700),
                    ..typed(None, None, None)
                },
                // A blocked discount counts as none.
                typed(
                    Some(10_000),
                    None,
                    Some(ItemDiscount::Fixed { cents: 20_000 }),
                ),
            ],
        })
        .expect("in range");
        assert_eq!(
            preview.item_profits_cents,
            vec![125_000, -400, -700, 10_000]
        );
        assert_eq!(preview.totals.cost_cents, 101_600);
        assert_eq!(
            preview.totals.price_cents - preview.totals.cost_cents,
            preview.item_profits_cents.iter().sum::<i64>()
        );
    }

    #[test]
    fn adds_labour_and_parts_into_the_price_and_parts_into_the_cost() {
        let totals = repair_totals(&[item(250_000, 1_200_000, 850_000), item(150_000, 0, 0)])
            .expect("in range");
        assert_eq!(
            totals,
            RepairTotals {
                labor_cents: 400_000,
                parts_cents: 1_200_000,
                discount_cents: 0,
                price_cents: 1_600_000,
                cost_cents: 850_000,
            }
        );
    }

    #[test]
    fn discounts_come_off_the_price_but_not_off_the_gross_sums() {
        let totals = repair_totals(&[
            discounted(0, 1_200_000, percent(1_000)),
            discounted(250_000, 0, ItemDiscount::Fixed { cents: 50_000 }),
        ])
        .expect("in range");
        assert_eq!(totals.parts_cents, 1_200_000);
        assert_eq!(totals.labor_cents, 250_000);
        assert_eq!(totals.discount_cents, 170_000);
        assert_eq!(totals.price_cents, 1_450_000 - 170_000);
    }

    #[test]
    fn no_items_is_a_free_job() {
        let totals = repair_totals(&[]).expect("in range");
        assert_eq!(totals.price_cents, 0);
        assert_eq!(totals.cost_cents, 0);
    }

    #[test]
    fn refuses_negative_amounts_and_too_many_items() {
        assert!(repair_totals(&[item(-1, 0, 0)]).is_err());
        assert!(repair_totals(&[item(0, 0, MAX_CENTS + 1)]).is_err());
        let many = vec![item(1, 0, 0); MAX_REPAIR_ITEMS + 1];
        assert!(repair_totals(&many).is_err());
        assert!(repair_totals(&[item(MAX_CENTS, MAX_CENTS, 0)]).is_err());
    }

    #[test]
    fn a_percentage_rounds_half_up_and_a_whole_one_takes_everything() {
        // 12.5% of 1.01 is 12.625 cents -> 13.
        assert_eq!(
            item_discount_cents(&discounted(101, 0, percent(1_250))),
            Ok(13)
        );
        // 0.01% of 100.00 is exactly 1 cent.
        assert_eq!(
            item_discount_cents(&discounted(10_000, 0, percent(1))),
            Ok(1)
        );
        assert_eq!(
            item_discount_cents(&discounted(0, 5_000, percent(10_000))),
            Ok(5_000)
        );
        assert_eq!(item_discount_cents(&item(0, 5_000, 0)), Ok(0));
    }

    #[test]
    fn a_fixed_discount_can_be_the_whole_price_but_no_more() {
        let fixed = |cents| ItemDiscount::Fixed { cents };
        assert_eq!(
            item_discount_cents(&discounted(0, 5_000, fixed(5_000))),
            Ok(5_000)
        );
        assert!(item_discount_cents(&discounted(0, 5_000, fixed(5_001))).is_err());
        assert!(item_discount_cents(&discounted(0, 0, fixed(1))).is_err());
    }

    #[test]
    fn refuses_a_discount_out_of_range_or_on_an_item_with_a_fee_and_a_part() {
        assert!(item_discount_cents(&discounted(5_000, 0, percent(0))).is_err());
        assert!(item_discount_cents(&discounted(5_000, 0, percent(10_001))).is_err());
        assert!(
            item_discount_cents(&discounted(5_000, 0, ItemDiscount::Fixed { cents: 0 })).is_err()
        );
        assert!(item_discount_cents(&discounted(5_000, 5_000, percent(1_000))).is_err());
        assert!(item_discount_cents(&discounted(0, 0, percent(1_000))).is_err());
        assert!(repair_totals(&[discounted(5_000, 5_000, percent(1_000))]).is_err());
    }

    #[test]
    fn a_half_typed_item_counts_its_missing_amounts_as_nothing() {
        let preview = compute(&CalcRepairRequest {
            items: vec![typed(Some(50_000), None, None)],
        })
        .expect("in range");
        assert_eq!(preview.totals.price_cents, 50_000);
        assert_eq!(preview.totals.cost_cents, 0);
        assert_eq!(preview.item_discounts_cents, vec![0]);
        assert!(preview.blockers.is_empty());
    }

    #[test]
    fn the_preview_works_out_each_discount_and_blocks_one_that_cannot_stand() {
        let preview = compute(&CalcRepairRequest {
            items: vec![
                typed(None, Some(1_200_000), Some(percent(1_000))),
                typed(
                    Some(10_000),
                    None,
                    Some(ItemDiscount::Fixed { cents: 20_000 }),
                ),
                typed(Some(5_000), Some(5_000), Some(percent(500))),
                typed(None, None, Some(percent(500))),
            ],
        })
        .expect("in range");
        assert_eq!(preview.item_discounts_cents, vec![120_000, 0, 0, 0]);
        assert_eq!(
            preview.blockers,
            vec![
                RepairBlocker {
                    item_index: 1,
                    code: RepairBlockerCode::DiscountMoreThanItem
                },
                RepairBlocker {
                    item_index: 2,
                    code: RepairBlockerCode::DiscountOnMixedItem
                },
                RepairBlocker {
                    item_index: 3,
                    code: RepairBlockerCode::DiscountMoreThanItem
                },
            ]
        );
        assert_eq!(preview.totals.discount_cents, 120_000);
        assert_eq!(
            preview.totals.price_cents,
            1_200_000 + 10_000 + 10_000 - 120_000
        );
    }

    #[test]
    fn the_preview_refuses_a_percentage_the_popover_can_never_send() {
        let request = CalcRepairRequest {
            items: vec![typed(Some(5_000), None, Some(percent(20_000)))],
        };
        assert!(compute(&request).is_err());
    }
}
