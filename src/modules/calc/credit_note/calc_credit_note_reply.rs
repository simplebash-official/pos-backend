use super::calc_refund_leg::CalcRefundLeg;
use super::calc_returned_line_reply::CalcReturnedLineReply;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcCreditNoteReply {
    pub lines: Vec<CalcReturnedLineReply>,
    pub return_subtotal_cents: i64,
    pub exchange_subtotal_cents: i64,
    /// Positive: the customer is owed this; negative: the customer owes it.
    pub net_refund_cents: i64,
    /// The part of the net that goes back as cash (capped at what was paid on the invoice).
    pub refund_cash_cents: i64,
    /// The part that only reduces what the customer owes.
    pub balance_reduction_cents: i64,
    /// Where the cash goes by default: in proportion to the invoice's payments when it was paid in
    /// several, otherwise one leg.
    pub refund_legs: Vec<CalcRefundLeg>,
    pub is_refund_due: bool,
    pub is_customer_owing: bool,
    pub is_even_exchange: bool,
    /// Whole days since the sale; absent for a return with no receipt.
    pub days_since_sale: Option<i64>,
    pub return_window_days: i64,
    /// True when the sale is older than the return window, which needs an administrator's approval.
    pub past_return_window: bool,
}
