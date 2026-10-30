use super::CalcSharesReply;
use super::CalcSharesRequest;
use crate::core::error::AppResult;

pub fn compute(request: &CalcSharesRequest) -> AppResult<CalcSharesReply> {
    Ok(pos_calc_core::shares::compute(request)?)
}
