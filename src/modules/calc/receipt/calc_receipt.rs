use super::{CalcReceiptLineReply, CalcReceiptReply, CalcReceiptRequest};
use crate::core::error::AppResult;

pub type CalcLineReply = CalcReceiptLineReply;

/// Computes the receipt figures with the shared calculation crate.
pub fn compute(request: &CalcReceiptRequest) -> AppResult<CalcReceiptReply> {
    Ok(pos_calc_core::receipt::compute(request)?)
}

pub const MAX_CENTS: i64 = 10_000_000_000_000;

pub const MAX_QUANTITY: i64 = 1_000_000_000;
