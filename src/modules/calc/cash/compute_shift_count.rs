use super::calc_shift_count_reply::CalcShiftCountReply;
use super::calc_shift_count_request::CalcShiftCountRequest;
use super::check_cents::check_cents;
use crate::core::error::{AppError, AppResult};
use crate::domain::sessions::VARIANCE_TOLERANCE_CENTS;

/// `opening_float_cents` is the shift's opening float, known only when closing.
pub fn compute_shift_count(
    request: &CalcShiftCountRequest,
    expected_cents: i64,
    opening_float_cents: Option<i64>,
) -> AppResult<CalcShiftCountReply> {
    check_cents("The count", request.counted_cents)?;
    let counted = request.counted_cents;
    let variance_cents = counted - expected_cents;
    let (closing_float_cents, keep_opening_float_cents, safe_deposit_cents) =
        match (opening_float_cents, request.closing_float_cents) {
            (Some(opening), chosen) => {
                let keep = counted.min(opening);
                let float = chosen.unwrap_or(keep);
                check_cents("The float", float)?;
                if float > counted {
                    return Err(AppError::validation(
                        "The float cannot be more than the count",
                    ));
                }
                (Some(float), Some(keep), Some(counted - float))
            }
            (None, Some(_)) => {
                return Err(AppError::validation("A float is only left when closing"));
            }
            (None, None) => (None, None, None),
        };
    Ok(CalcShiftCountReply {
        counted_cents: counted,
        expected_cents,
        variance_cents,
        reason_required: variance_cents != 0,
        pin_required: if opening_float_cents.is_some() {
            variance_cents.abs() > VARIANCE_TOLERANCE_CENTS
        } else {
            variance_cents != 0
        },
        variance_tolerance_cents: VARIANCE_TOLERANCE_CENTS,
        closing_float_cents,
        keep_opening_float_cents,
        safe_deposit_cents,
    })
}
