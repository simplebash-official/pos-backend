#[derive(Debug, Clone, Default)]
pub(crate) struct OutstandingStatsAggregate {
    pub total_outstanding_cents: i64,
    pub total_credit_invoices_count: u64,
    pub overdue_invoices_count: u64,
    pub overdue_amount_cents: i64,
}
