use crate::core::error::{AppError, AppResult};
use crate::domain::sessions::SessionStatus;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftTotalsRequest {
    pub session_keys: Vec<String>,
}

const MAX_SHIFTS: usize = 200;

/// Rejects a request naming too many shifts.
pub fn check(request: &CalcShiftTotalsRequest) -> AppResult<()> {
    if request.session_keys.len() > MAX_SHIFTS {
        return Err(AppError::validation("Too many shifts"));
    }
    Ok(())
}

#[must_use]
/// Adds up the figures of several shifts.
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

/// What the totals need from one shift.
#[derive(Debug)]
pub struct ShiftFigures {
    pub status: SessionStatus,
    pub cash_sales_cents: i64,
    pub cash_variance_cents: Option<i64>,
}
