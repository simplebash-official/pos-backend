use super::CalcWarrantyReply;
use super::CalcWarrantyRequest;
use crate::core::error::AppResult;

/// Works out the warranty period with the shared calculation crate.
pub fn compute(request: &CalcWarrantyRequest) -> AppResult<CalcWarrantyReply> {
    Ok(pos_calc_core::warranty::compute(request)?)
}
