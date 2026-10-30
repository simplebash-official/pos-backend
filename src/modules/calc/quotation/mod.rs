//! The quotation form's live figures: what each line's discount comes to, each line's total, the
//! subtotal, the order discount and the total, plus what still stops it being created. Pure; the
//! maths is `pos_calc_core::quotation`.

pub use pos_calc_core::quotation::{
    CalcQuotationLine, CalcQuotationLineDiscount, CalcQuotationLineReply,
    CalcQuotationOrderDiscount, CalcQuotationReply, CalcQuotationRequest, QuotationBlocker,
    QuotationBlockerCode, QuotationDiscountScope, QuotationDiscountType,
};
mod compute;
#[cfg(test)]
mod tests;

pub use compute::compute;
