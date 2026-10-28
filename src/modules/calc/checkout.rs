// The checkout screen's live figures: every cart line, the totals, the order discount, the
// payment state (change, short by, split, credit) and the quick-tender buttons. Pure: the maths is `pos_calc_core`.
//
// A box still empty is 0 or absent; nothing is defaulted on the way. Malformed or out-of-range
// input is a 400.

use crate::core::error::AppResult;

pub use pos_calc_core::checkout::{
    CalcCartLine, CalcCheckoutPayment, CalcCheckoutReply, CalcCheckoutRequest, CalcDiscount,
    CalcDiscountType, CalcLineReply, CalcPaymentMethod, CalcPaymentReply, CalcSource,
    CalcSourceBreakdown,
};

pub fn compute_checkout(req: &CalcCheckoutRequest) -> AppResult<CalcCheckoutReply> {
    Ok(pos_calc_core::checkout::compute_checkout(req)?)
}
