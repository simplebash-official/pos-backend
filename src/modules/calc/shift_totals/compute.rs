use super::calc_shift_totals_reply::CalcShiftTotalsReply;
use super::shift_figures::ShiftFigures;

use crate::domain::sessions::SessionStatus;

#[must_use]
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
