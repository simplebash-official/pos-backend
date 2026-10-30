use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /billing/invoices/stats` — the 4 KPI cards on the
/// frontend's Sales & Invoices History screen, computed server-side so
/// online and offline clients always see the same numbers.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BillingStats {
    /// Sum of `totalCents` across invoices created today (UTC day boundary).
    pub today_sales_cents: i64,
    /// Count of invoices created today.
    pub today_invoice_count: u64,
    /// Sum of `totalCents` across ALL invoices (any date) that are either
    /// on credit (`isCredit == true`) or still `status` in (pending,
    /// partially_paid).
    pub outstanding_credit_cents: i64,
    /// `todaySalesCents / todayInvoiceCount`, rounded; 0 when there were no
    /// invoices today.
    pub avg_basket_cents: i64,
}
