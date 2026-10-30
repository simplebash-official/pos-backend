use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CalcShiftTotalsReply {
    pub shift_count: i64,
    /// Open, closing or reopened: still counting sales.
    pub running_count: i64,
    pub cash_sales_cents: i64,
    /// Net cash difference of the closed shifts: over is positive, short is negative.
    pub net_variance_cents: i64,
}
