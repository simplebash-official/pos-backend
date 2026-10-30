use serde::Serialize;
use utoipa::ToSchema;

/// Cash reconciliation summary for Z-Report.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportCashReconciliation {
    pub opening_float_cents: i64,
    pub cash_sales_cents: i64,
    pub pay_in_cents: i64,
    pub pay_out_cents: i64,
    pub cash_refunds_cents: i64,
    pub owner_withdrawals_cents: i64,
    pub cash_drops_cents: i64,
    pub expected_cash_cents: i64,
    pub counted_cash_cents: i64,
    pub variance_cents: i64,
    pub closing_float_cents: i64,
    pub safe_deposit_cents: i64,
}
