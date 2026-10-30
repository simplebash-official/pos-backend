/// Everything `service::get_repair_stats` needs from `repairs`, computed in
/// one aggregation round-trip.
#[derive(Debug)]
pub(crate) struct StatsAggregateResult {
    /// Count of non-deleted tickets created in `[today_start, today_end)`.
    pub today_job_count: u64,
    /// Sum of `estimated_cost_cents` (0 where unset) for those same tickets.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted tickets (any date) whose status is neither
    /// "delivered" nor "cancelled".
    pub pending_job_count: u64,
}
