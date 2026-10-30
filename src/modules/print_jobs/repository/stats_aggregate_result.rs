/// Everything `service::get_print_job_stats` needs from `print_jobs`,
/// computed in one aggregation round-trip.
#[derive(Debug)]
pub(crate) struct StatsAggregateResult {
    /// Count of non-deleted jobs created in `[today_start, today_end)`.
    pub today_job_count: u64,
    /// Sum of `estimated_cost_cents` for those same jobs.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted jobs (any date) whose status is neither
    /// "delivered" nor "cancelled".
    pub pending_job_count: u64,
}
