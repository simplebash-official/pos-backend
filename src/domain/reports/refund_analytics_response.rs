use super::refund_method_row::RefundMethodRow;
use super::refund_product_row::RefundProductRow;
use super::refund_reason_row::RefundReasonRow;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundAnalyticsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub credit_note_count: u64,
    pub net_refund_cents: i64,
    pub refund_cash_cents: i64,
    pub balance_reduction_cents: i64,
    /// Net refunds as a share of period revenue, in basis points.
    pub refund_rate_bps: i64,
    pub exchange_count: u64,
    pub no_receipt_count: u64,
    pub manager_override_count: u64,
    pub by_reason: Vec<RefundReasonRow>,
    pub by_method: Vec<RefundMethodRow>,
    pub top_returned_products: Vec<RefundProductRow>,
}
