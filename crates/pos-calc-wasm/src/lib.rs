use wasm_bindgen::prelude::*;

pub use pos_calc_core::{
    cash::{CalcCashCountReply, CalcCashCountRequest, CashCountLine, DENOMINATION_RUPEES},
    checkout::{
        CalcCartLine, CalcCheckoutPayment, CalcCheckoutReply, CalcCheckoutRequest, CalcDiscount,
        CalcDiscountType, CalcLineReply, CalcPaymentMethod, CalcPaymentReply, CalcSource,
        CalcSourceBreakdown,
    },
    earnings::{CalcEarningsReply, CalcEarningsRequest, SplitKind},
    quotation::{
        CalcQuotationLine, CalcQuotationLineDiscount, CalcQuotationLineReply,
        CalcQuotationOrderDiscount, CalcQuotationReply, CalcQuotationRequest, QuotationBlocker,
        QuotationBlockerCode, QuotationDiscountScope, QuotationDiscountType,
    },
    receipt::{
        Allocation, BilledBy, Blocker, BlockerCode, CalcCharge, CalcChargesPayment, CalcLine,
        CalcMargin, CalcPayment, CalcReceiptLineReply, CalcReceiptReply, CalcReceiptRequest,
        CalcReceiptSummary, ChargeKind, CostMode, PaymentMode, PaymentSource, PricingMode,
    },
    shares::{CalcSharesReply, CalcSharesRequest, ShareLine, SharePart},
};

fn to_js_err(err: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&err.to_string())
}

#[wasm_bindgen]
pub fn calc_checkout(request: CalcCheckoutRequest) -> Result<CalcCheckoutReply, JsValue> {
    pos_calc_core::checkout::compute_checkout(&request).map_err(to_js_err)
}

#[wasm_bindgen]
pub fn calc_receipt(request: CalcReceiptRequest) -> Result<CalcReceiptReply, JsValue> {
    pos_calc_core::receipt::compute(&request).map_err(to_js_err)
}

#[wasm_bindgen]
pub fn calc_quotation(request: CalcQuotationRequest) -> Result<CalcQuotationReply, JsValue> {
    let now = js_sys::Date::new_0();
    let year = now.get_full_year() as i32;
    let month = (now.get_month() + 1) as u32;
    let day = now.get_date() as u32;
    let today = chrono::NaiveDate::from_ymd_opt(year, month, day)
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    pos_calc_core::quotation::compute(&request, today).map_err(to_js_err)
}

#[wasm_bindgen]
pub fn calc_earnings(request: CalcEarningsRequest) -> Result<CalcEarningsReply, JsValue> {
    pos_calc_core::earnings::compute(&request).map_err(to_js_err)
}

#[wasm_bindgen]
pub fn calc_cash_count(request: CalcCashCountRequest) -> Result<CalcCashCountReply, JsValue> {
    pos_calc_core::cash::compute_count(&request).map_err(to_js_err)
}

#[wasm_bindgen]
pub fn calc_shares(request: CalcSharesRequest) -> Result<CalcSharesReply, JsValue> {
    pos_calc_core::shares::compute(&request).map_err(to_js_err)
}
