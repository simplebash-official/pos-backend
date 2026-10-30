use super::CalcEarningsReply;
use super::CalcEarningsRequest;
use crate::core::error::AppResult;

/// Computes the earnings figures with the shared calculation crate.
pub fn compute(request: &CalcEarningsRequest) -> AppResult<CalcEarningsReply> {
    Ok(pos_calc_core::earnings::compute(request)?)
}
