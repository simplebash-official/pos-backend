// The figures over the shifts a list is showing: how many, how many are still running, the cash
// sales together and the net cash difference of the closed ones. Pure; the handler reads the shifts
// by key, so the totals can only be of real shifts and the list's own filters stay on the screen.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    core::error::{AppError, AppResult},
    domain::sessions::SessionStatus,
};

const MAX_SHIFTS: usize = 200;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftTotalsRequest {
    pub session_keys: Vec<String>,
}

/// What the totals need from one shift.
pub struct ShiftFigures {
    pub status: SessionStatus,
    pub cash_sales_cents: i64,
    pub cash_variance_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftTotalsReply {
    pub shift_count: i64,
    /// Open, closing or reopened: still counting sales.
    pub running_count: i64,
    pub cash_sales_cents: i64,
    /// Net cash difference of the closed shifts: over is positive, short is negative.
    pub net_variance_cents: i64,
}

pub fn check(request: &CalcShiftTotalsRequest) -> AppResult<()> {
    if request.session_keys.len() > MAX_SHIFTS {
        return Err(AppError::validation("Too many shifts"));
    }
    Ok(())
}

pub fn compute(shifts: &[ShiftFigures]) -> CalcShiftTotalsReply {
    CalcShiftTotalsReply {
        shift_count: shifts.len() as i64,
        running_count: shifts
            .iter()
            .filter(|s| s.status != SessionStatus::Closed)
            .count() as i64,
        cash_sales_cents: shifts.iter().map(|s| s.cash_sales_cents).sum(),
        net_variance_cents: shifts
            .iter()
            .filter(|s| s.status == SessionStatus::Closed)
            .filter_map(|s| s.cash_variance_cents)
            .sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shift(status: SessionStatus, cash: i64, variance: Option<i64>) -> ShiftFigures {
        ShiftFigures { status, cash_sales_cents: cash, cash_variance_cents: variance }
    }

    #[test]
    fn only_closed_shifts_count_towards_the_net_difference() {
        let reply = compute(&[
            shift(SessionStatus::Closed, 10_000, Some(-300)),
            shift(SessionStatus::Closed, 5_000, Some(100)),
            shift(SessionStatus::Open, 2_500, None),
            shift(SessionStatus::Reopened, 1_000, Some(900)),
        ]);
        assert_eq!(reply.shift_count, 4);
        assert_eq!(reply.running_count, 2);
        assert_eq!(reply.cash_sales_cents, 18_500);
        assert_eq!(reply.net_variance_cents, -200);
    }
}
