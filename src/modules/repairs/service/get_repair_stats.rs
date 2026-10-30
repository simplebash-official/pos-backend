use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::core::utils::shop_today_range;
use crate::domain::repairs::RepairStats;

use crate::modules::repairs::repository;

/// Computes the KPI cards for the Repair Jobs screen. See `shop_today_range`
/// for how "today" is bounded.
pub async fn get_repair_stats(db: &Db) -> AppResult<RepairStats> {
    let (today_start, today_end) = shop_today_range();
    let agg = repository::aggregate_stats(db, today_start, today_end).await?;

    let avg_job_value_cents = if agg.today_job_count > 0 {
        (agg.today_revenue_cents as f64 / agg.today_job_count as f64).round() as i64
    } else {
        0
    };

    Ok(RepairStats {
        today_job_count: agg.today_job_count,
        today_revenue_cents: agg.today_revenue_cents,
        pending_job_count: agg.pending_job_count,
        avg_job_value_cents,
    })
}
