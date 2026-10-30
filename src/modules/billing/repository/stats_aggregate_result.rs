/// Everything `service::get_billing_stats` needs from `invoices`, computed in
/// one aggregation round-trip.
#[derive(Debug)]
pub(crate) struct StatsAggregateResult {
    /// Sum of `total_cents` for invoices created in `[today_start, today_end)`.
    pub today_sales_cents: i64,
    /// Count of invoices created in that same window.
    pub today_invoice_count: u64,
    /// Sum of `total_cents` across ALL invoices, unscoped by date, where
    /// `is_credit == true` or `status` is `pending`/`partially_paid`.
    pub outstanding_credit_cents: i64,
}
