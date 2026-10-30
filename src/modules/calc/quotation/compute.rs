use super::CalcQuotationReply;
use super::CalcQuotationRequest;
use crate::core::error::AppResult;
use chrono::NaiveDate;

/// Computes the totals and validity of a quotation as of `today`.
pub fn compute(req: &CalcQuotationRequest, today: NaiveDate) -> AppResult<CalcQuotationReply> {
    Ok(pos_calc_core::quotation::compute(req, today)?)
}
