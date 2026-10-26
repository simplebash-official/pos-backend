// What a repair or print job earns and what the assigned employee's commission comes to, for the
// job forms and the employee screen's sample. Pure; the maths is `core::calculations`, the same
// code the commission reports use, so what a form promises is what the reports show.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::core::{
    calculations::{commission_cents, job_profit_cents},
    error::{AppError, AppResult},
};

const MAX_CENTS: i64 = 10_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitKind {
    /// A share of the job's profit; the value is the percent (0-100).
    Percentage,
    /// A flat amount; the value is cents.
    Fixed,
}

impl SplitKind {
    fn as_str(self) -> &'static str {
        match self {
            SplitKind::Percentage => "percentage",
            SplitKind::Fixed => "fixed",
        }
    }
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
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

pub fn compute(request: &CalcEarningsRequest) -> AppResult<CalcEarningsReply> {
    let in_range = |cents: i64| (0..=MAX_CENTS).contains(&cents);
    if !in_range(request.material_cost_cents)
        || request.price_cents.is_some_and(|price| !in_range(price))
    {
        return Err(AppError::validation("An amount is out of range"));
    }
    let split_ok = request.split_value.is_finite()
        && request.split_value >= 0.0
        && match request.split_type {
            Some(SplitKind::Percentage) => request.split_value <= 100.0,
            _ => request.split_value <= MAX_CENTS as f64,
        };
    if !split_ok {
        return Err(AppError::validation("The commission is out of range"));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        price: Option<i64>,
        material: i64,
        kind: Option<SplitKind>,
        value: f64,
    ) -> CalcEarningsRequest {
        CalcEarningsRequest {
            price_cents: price,
            material_cost_cents: material,
            split_type: kind,
            split_value: value,
        }
    }

    #[test]
    fn a_percentage_is_taken_of_the_profit() {
        // Rs 10,000 job, Rs 4,000 of parts: Rs 6,000 profit, 30% is Rs 1,800.
        let reply = compute(&request(Some(1_000_000), 400_000, Some(SplitKind::Percentage), 30.0))
            .unwrap();
        assert_eq!((reply.profit_cents, reply.commission_cents), (600_000, 180_000));
        assert_eq!(reply.shop_keeps_cents, 420_000);
        assert!(!reply.capped);
    }

    #[test]
    fn a_fixed_commission_is_cut_down_to_the_profit() {
        let reply =
            compute(&request(Some(100_000), 70_000, Some(SplitKind::Fixed), 50_000.0)).unwrap();
        assert_eq!((reply.profit_cents, reply.commission_cents), (30_000, 30_000));
        assert!(reply.capped);
        assert_eq!(reply.shop_keeps_cents, 0);
        let enough =
            compute(&request(Some(100_000), 70_000, Some(SplitKind::Fixed), 20_000.0)).unwrap();
        assert_eq!(enough.commission_cents, 20_000);
        assert!(!enough.capped);
    }

    #[test]
    fn a_loss_earns_no_commission_and_no_split_means_none() {
        let loss = compute(&request(Some(50_000), 80_000, Some(SplitKind::Fixed), 10_000.0)).unwrap();
        assert_eq!((loss.profit_cents, loss.commission_cents, loss.capped), (0, 0, false));
        let none = compute(&request(Some(50_000), 0, None, 0.0)).unwrap();
        assert_eq!((none.profit_cents, none.commission_cents), (50_000, 0));
    }

    #[test]
    fn nothing_is_worked_out_until_a_price_is_typed() {
        let reply = compute(&request(None, 1_000, Some(SplitKind::Percentage), 30.0)).unwrap();
        assert!(!reply.price_entered);
        assert_eq!(reply.commission_cents, 0);
    }

    #[test]
    fn half_cents_round_up_and_bad_values_are_refused() {
        // 12.5% of Rs 0.10 is 1.25 cents -> 1.
        let reply = compute(&request(Some(10), 0, Some(SplitKind::Percentage), 12.5)).unwrap();
        assert_eq!(reply.commission_cents, 1);
        assert!(compute(&request(Some(10), 0, Some(SplitKind::Percentage), 101.0)).is_err());
        assert!(compute(&request(Some(-1), 0, None, 0.0)).is_err());
        assert!(compute(&request(Some(10), 0, Some(SplitKind::Fixed), f64::NAN)).is_err());
    }
}
