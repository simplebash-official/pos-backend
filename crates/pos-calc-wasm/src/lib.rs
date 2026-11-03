//! WebAssembly entry points for the point-of-sale calculations. Each function takes the same
//! request the backend's calculation endpoint takes and runs the same `pos-calc-core` code, so
//! the browser and the server cannot disagree.

use chrono::NaiveDate;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

pub use pos_calc_core::cash::{
    CalcCashCountReply, CalcCashCountRequest, CashCountLine, DENOMINATION_RUPEES,
};

pub use pos_calc_core::checkout::{
    CalcCartLine, CalcCheckoutPayment, CalcCheckoutReply, CalcCheckoutRequest, CalcDiscount,
    CalcDiscountType, CalcLineReply, CalcPaymentMethod, CalcPaymentReply, CalcSource,
    CalcSourceBreakdown,
};

pub use pos_calc_core::earnings::{CalcEarningsReply, CalcEarningsRequest, SplitKind};

pub use pos_calc_core::quotation::{
    CalcQuotationLine, CalcQuotationLineDiscount, CalcQuotationLineReply,
    CalcQuotationOrderDiscount, CalcQuotationReply, CalcQuotationRequest, QuotationBlocker,
    QuotationBlockerCode, QuotationDiscountScope, QuotationDiscountType,
};

pub use pos_calc_core::receipt::{
    Allocation, BilledBy, Blocker, BlockerCode, CalcCharge, CalcChargesPayment, CalcLine,
    CalcMargin, CalcPayment, CalcReceiptLineReply, CalcReceiptReply, CalcReceiptRequest,
    CalcReceiptSummary, ChargeKind, CostMode, PaymentMode, PaymentSource, PricingMode,
};

pub use pos_calc_core::shares::{CalcSharesReply, CalcSharesRequest, ShareLine, SharePart};

pub use pos_calc_core::warranty::{
    CalcWarrantyReply, CalcWarrantyRequest, WarrantyCoverage, WarrantyUnit,
};

/// Turns a calculation error into the message the browser shows.
fn to_js_err(err: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&err.to_string())
}

/// The calendar date for a JavaScript `Date`'s year, zero-based month and day of month.
fn calendar_date(year: i32, zero_based_month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, zero_based_month + 1, day)
        .expect("a JavaScript Date always has a valid calendar date")
}

/// Works out a cart and its payment.
#[wasm_bindgen]
pub fn calc_checkout(request: CalcCheckoutRequest) -> Result<CalcCheckoutReply, JsValue> {
    pos_calc_core::checkout::compute_checkout(&request).map_err(to_js_err)
}

/// Works out a stock receipt.
#[wasm_bindgen]
pub fn calc_receipt(request: CalcReceiptRequest) -> Result<CalcReceiptReply, JsValue> {
    pos_calc_core::receipt::compute(&request).map_err(to_js_err)
}

/// Works out a quotation; its validity date is checked against today in the browser.
#[wasm_bindgen]
pub fn calc_quotation(request: CalcQuotationRequest) -> Result<CalcQuotationReply, JsValue> {
    let now = js_sys::Date::new_0();
    let today = calendar_date(now.get_full_year() as i32, now.get_month(), now.get_date());
    pos_calc_core::quotation::compute(&request, today).map_err(to_js_err)
}

/// Works out what a repair job earns and the commission on it.
#[wasm_bindgen]
pub fn calc_earnings(request: CalcEarningsRequest) -> Result<CalcEarningsReply, JsValue> {
    pos_calc_core::earnings::compute(&request).map_err(to_js_err)
}

/// Totals a cash count by denomination.
#[wasm_bindgen]
pub fn calc_cash_count(request: CalcCashCountRequest) -> Result<CalcCashCountReply, JsValue> {
    pos_calc_core::cash::compute_count(&request).map_err(to_js_err)
}

/// Works out each part's share of its whole.
#[wasm_bindgen]
pub fn calc_shares(request: CalcSharesRequest) -> Result<CalcSharesReply, JsValue> {
    pos_calc_core::shares::compute(&request).map_err(to_js_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_date_reads_the_zero_based_month_of_a_javascript_date() {
        assert_eq!(
            calendar_date(2026, 0, 31),
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("a real date")
        );
        assert_eq!(
            calendar_date(2026, 11, 1),
            NaiveDate::from_ymd_opt(2026, 12, 1).expect("a real date")
        );
    }

    #[test]
    #[should_panic(expected = "valid calendar date")]
    fn calendar_date_refuses_a_day_that_does_not_exist() {
        let _ = calendar_date(2026, 1, 30);
    }
}

/// Works out the last day a warranty starting on a given day covers.
#[wasm_bindgen]
pub fn calc_warranty(request: CalcWarrantyRequest) -> Result<CalcWarrantyReply, JsValue> {
    pos_calc_core::warranty::compute(&request).map_err(to_js_err)
}
