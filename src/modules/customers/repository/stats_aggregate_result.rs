#[derive(Debug)]
pub(crate) struct StatsAggregateResult {
    pub total_customers: u64,
    pub total_balance_due_cents: i64,
    pub active_debtors_count: u64,
}
