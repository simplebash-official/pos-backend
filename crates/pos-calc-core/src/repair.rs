//! What a repair job charges and costs, worked out from its items. Each item is a piece of
//! work: labour, plus at most one part with the price the customer pays for it and what the
//! part cost the shop. The job's price is all the labour and part prices; its cost is the
//! part costs.

use serde::{Deserialize, Serialize};

use crate::error::{CalcError, CalcResult};

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
    /// What the customer pays for the part.
    pub part_cents: Option<i64>,
    /// What the part cost the shop.
    pub part_cost_cents: Option<i64>,
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
    /// All the labour.
    pub labor_cents: i64,
    /// All the part prices.
    pub parts_cents: i64,
    /// What the customer pays: labour and parts.
    pub price_cents: i64,
    /// What the parts cost the shop.
    pub cost_cents: i64,
}

/// One item's money once every field is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairItemMoney {
    /// What the work itself is charged.
    pub labor_cents: i64,
    /// What the customer pays for the part.
    pub part_cents: i64,
    /// What the part cost the shop.
    pub part_cost_cents: i64,
}

/// Adds up a job's items.
///
/// # Errors
///
/// Fails when there are too many items, an amount is negative or too large, or a total does
/// not fit.
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
    for item in items {
        if !in_range(item.labor_cents)
            || !in_range(item.part_cents)
            || !in_range(item.part_cost_cents)
        {
            return Err(CalcError::validation(
                "A repair amount cannot be negative or that large.",
            ));
        }
        labor += i128::from(item.labor_cents);
        parts += i128::from(item.part_cents);
        cost += i128::from(item.part_cost_cents);
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
        price_cents: fit(labor + parts)?,
        cost_cents: fit(cost)?,
    })
}

/// Works out the totals of a job still being typed; a missing amount counts as nothing.
///
/// # Errors
///
/// Fails like [`repair_totals`].
pub fn compute(request: &CalcRepairRequest) -> CalcResult<RepairTotals> {
    let items: Vec<RepairItemMoney> = request
        .items
        .iter()
        .map(|item| RepairItemMoney {
            labor_cents: item.labor_cents.unwrap_or(0),
            part_cents: item.part_cents.unwrap_or(0),
            part_cost_cents: item.part_cost_cents.unwrap_or(0),
        })
        .collect();
    repair_totals(&items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(labor: i64, part: i64, cost: i64) -> RepairItemMoney {
        RepairItemMoney {
            labor_cents: labor,
            part_cents: part,
            part_cost_cents: cost,
        }
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
                price_cents: 1_600_000,
                cost_cents: 850_000,
            }
        );
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
    fn a_half_typed_item_counts_its_missing_amounts_as_nothing() {
        let totals = compute(&CalcRepairRequest {
            items: vec![CalcRepairItem {
                labor_cents: Some(50_000),
                part_cents: None,
                part_cost_cents: None,
            }],
        })
        .expect("in range");
        assert_eq!(totals.price_cents, 50_000);
        assert_eq!(totals.cost_cents, 0);
    }
}
