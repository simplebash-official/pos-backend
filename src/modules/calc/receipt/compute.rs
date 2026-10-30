use super::CalcReceiptReply;
use super::CalcReceiptRequest;
use crate::core::error::AppResult;

pub fn compute(request: &CalcReceiptRequest) -> AppResult<CalcReceiptReply> {
    Ok(pos_calc_core::receipt::compute(request)?)
}
