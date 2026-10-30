use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /print-jobs/stats` — the KPI cards on the frontend's
/// Print Jobs screen. Field names are byte-identical to `repairs::RepairStats`
/// (the frontend renders both through the same component) even though the
/// underlying collections differ.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobStats {
    /// Count of print jobs created today (UTC day boundary).
    pub today_job_count: u64,
    /// Sum of `estimatedCostCents` for jobs created today.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted jobs (any date) whose status is neither
    /// "delivered" nor "cancelled" — i.e. still open in the pipeline.
    pub pending_job_count: u64,
    /// `todayRevenueCents / todayJobCount`, rounded; 0 when no jobs were
    /// created today.
    pub avg_job_value_cents: i64,
}
