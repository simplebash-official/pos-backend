use super::calc_shift_totals_request::CalcShiftTotalsRequest;
use crate::core::error::{AppError, AppResult};

const MAX_SHIFTS: usize = 200;

pub fn check(request: &CalcShiftTotalsRequest) -> AppResult<()> {
    if request.session_keys.len() > MAX_SHIFTS {
        return Err(AppError::validation("Too many shifts"));
    }
    Ok(())
}
