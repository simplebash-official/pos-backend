use serde::{Deserialize, Serialize};

use crate::{
    calculations::{commission_cents, job_profit_cents},
    error::{CalcError, CalcResult},
};

const MAX_CENTS: i64 = 10_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub enum SplitKind {
    /// A share of the job's profit; the value is the percent (0-100).
    Percentage,
    /// A flat amount; the value is cents.
    Fixed,
}

impl SplitKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SplitKind::Percentage => "percentage",
            SplitKind::Fixed => "fixed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcEarningsRequest {
    /// The job's price; absent until one is typed.
    pub price_cents: Option<i64>,
    /// What the parts cost; 0 when nothing is typed.
    pub material_cost_cents: i64,
    /// Absent when no employee is assigned: no commission then.
    pub split_type: Option<SplitKind>,
    /// A percent (0-100) or, for a fixed split, cents.
    pub split_value: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
pub struct CalcEarningsReply {
    /// False until a price is typed; the figures below are 0 then.
    pub price_entered: bool,
    /// Price less parts, never below zero.
    pub profit_cents: i64,
    /// What the assigned employee earns.
    pub commission_cents: i64,
    /// A fixed commission bigger than the profit, so it is cut down to the profit.
    pub capped: bool,
    /// What the shop keeps after the commission.
    pub shop_keeps_cents: i64,
}

pub fn compute(request: &CalcEarningsRequest) -> CalcResult<CalcEarningsReply> {
    let in_range = |cents: i64| (0..=MAX_CENTS).contains(&cents);
    if !in_range(request.material_cost_cents)
        || request.price_cents.is_some_and(|price| !in_range(price))
    {
        return Err(CalcError::validation("An amount is out of range"));
    }
    let split_ok = request.split_value.is_finite()
        && request.split_value >= 0.0
        && match request.split_type {
            Some(SplitKind::Percentage) => request.split_value <= 100.0,
            _ => request.split_value <= MAX_CENTS as f64,
        };
    if !split_ok {
        return Err(CalcError::validation("The commission is out of range"));
    }

    let Some(price) = request.price_cents else {
        return Ok(CalcEarningsReply {
            price_entered: false,
            profit_cents: 0,
            commission_cents: 0,
            capped: false,
            shop_keeps_cents: 0,
        });
    };
    let profit = job_profit_cents(price, request.material_cost_cents);
    let commission = commission_cents(
        profit,
        request.split_type.map(SplitKind::as_str),
        request.split_value,
    );
    Ok(CalcEarningsReply {
        price_entered: true,
        profit_cents: profit,
        commission_cents: commission,
        capped: request.split_type == Some(SplitKind::Fixed)
            && profit > 0
            && request.split_value.round() as i64 > profit,
        shop_keeps_cents: profit - commission,
    })
}
