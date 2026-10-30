use super::CalcEarningsReply;
use super::CalcEarningsRequest;
use crate::core::error::AppResult;

pub fn compute(request: &CalcEarningsRequest) -> AppResult<CalcEarningsReply> {
    Ok(pos_calc_core::earnings::compute(request)?)
}
