use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /repairs/stats` — the KPI cards on the frontend's
/// Repair Jobs screen. Field names are byte-identical to `PrintJobStats`
/// (the frontend renders both through the same component) even though the
/// underlying collections differ.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairStats {
    /// Count of repair tickets created today (UTC day boundary).
    pub today_job_count: u64,
    /// Sum of `estimatedCostCents` (treated as 0 where unset/unquoted) for
    /// tickets created today.
    pub today_revenue_cents: i64,
    /// Count of ALL non-deleted tickets (any date) whose status is neither
    /// "delivered" nor "cancelled" — i.e. still open in the pipeline.
    pub pending_job_count: u64,
    /// `todayRevenueCents / todayJobCount`, rounded; 0 when no tickets were
    /// created today.
    pub avg_job_value_cents: i64,
}
