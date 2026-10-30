//! The Receive Stock form's live figures. Pure: no I/O. The money maths is `pos_calc_core::receipt`.

pub use pos_calc_core::receipt::{
    Allocation, BilledBy, Blocker, BlockerCode, CalcCharge, CalcChargesPayment, CalcLine,
    CalcMargin, CalcPayment, CalcReceiptLineReply, CalcReceiptReply, CalcReceiptRequest,
    CalcReceiptSummary, ChargeKind, CostMode, PaymentMode, PaymentSource, PricingMode,
};
mod calc_receipt;
#[cfg(test)]
mod tests;

pub use calc_receipt::CalcLineReply;
pub use calc_receipt::MAX_CENTS;
pub use calc_receipt::MAX_QUANTITY;
pub use calc_receipt::compute;
