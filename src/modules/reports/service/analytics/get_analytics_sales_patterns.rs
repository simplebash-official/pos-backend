use super::range::range;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{
    AnalyticsRangeQuery, PatternBucket, PatternCell, SalesPatternsResponse,
};
use crate::modules::reports::repository;

// ---------------------------------------------------------------------------
// sales patterns
// ---------------------------------------------------------------------------
const WEEKDAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub(crate) async fn get_analytics_sales_patterns(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<SalesPatternsResponse> {
    let (start, end) = range(&query)?;
    let raw = repository::analytics::aggregate_sales_patterns(db, start, end).await?;

    // Zero-filled 7×24 grid, row-major.
    let mut cells: Vec<PatternCell> = (1..=7u8)
        .flat_map(|w| {
            (0..24u8).map(move |h| PatternCell {
                weekday: w,
                hour: h,
                invoice_count: 0,
                revenue_cents: 0,
            })
        })
        .collect();

    let mut by_weekday = [(0u64, 0i64); 7];
    let mut by_hour = [(0u64, 0i64); 24];

    for (weekday, hour, count, revenue) in raw {
        let idx = ((weekday - 1) as usize) * 24 + hour as usize;
        if let Some(cell) = cells.get_mut(idx) {
            cell.invoice_count += count;
            cell.revenue_cents += revenue;
        }
        let w = &mut by_weekday[(weekday - 1) as usize];
        w.0 += count;
        w.1 += revenue;
        let h = &mut by_hour[hour as usize];
        h.0 += count;
        h.1 += revenue;
    }

    let busiest = cells
        .iter()
        .cloned()
        .max_by_key(|c| c.revenue_cents)
        .unwrap_or_default();

    Ok(SalesPatternsResponse {
        period_start: start,
        period_end: end,
        by_weekday: by_weekday
            .iter()
            .enumerate()
            .map(|(i, (count, rev))| PatternBucket {
                bucket: (i + 1) as u8,
                label: WEEKDAY_LABELS[i].to_string(),
                invoice_count: *count,
                revenue_cents: *rev,
            })
            .collect(),
        by_hour: by_hour
            .iter()
            .enumerate()
            .map(|(i, (count, rev))| PatternBucket {
                bucket: i as u8,
                label: format!("{i:02}:00"),
                invoice_count: *count,
                revenue_cents: *rev,
            })
            .collect(),
        cells,
        busiest,
    })
}
