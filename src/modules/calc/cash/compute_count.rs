use super::CalcCashCountReply;
use super::CalcCashCountRequest;
use crate::core::error::AppResult;

pub fn compute_count(request: &CalcCashCountRequest) -> AppResult<CalcCashCountReply> {
    Ok(pos_calc_core::cash::compute_count(request)?)
}
