use super::enriched_time_series_point::EnrichedTimeSeriesPoint;
use crate::domain::reports::TimeSeriesPoint;

/// Computes cumulative curves and a simple moving average (window = 7) over a time series.
#[must_use]
pub fn enrich_timeseries(
    points: &[TimeSeriesPoint],
    moving_avg_window: usize,
) -> Vec<EnrichedTimeSeriesPoint> {
    let mut enriched = Vec::with_capacity(points.len());
    let mut running_revenue: i64 = 0;
    let mut running_profit: i64 = 0;

    for (i, p) in points.iter().enumerate() {
        running_revenue += p.revenue_cents;
        running_profit += p.net_profit_cents;

        // Calculate moving average over the window
        let moving_avg = if i + 1 >= moving_avg_window && moving_avg_window > 0 {
            let start_idx = i + 1 - moving_avg_window;
            let sum: i64 = points[start_idx..=i]
                .iter()
                .map(|pt| pt.revenue_cents)
                .sum();
            Some(sum / moving_avg_window as i64)
        } else {
            None
        };

        enriched.push(EnrichedTimeSeriesPoint {
            point: p.clone(),
            cumulative_revenue_cents: running_revenue,
            cumulative_profit_cents: running_profit,
            moving_avg_revenue_cents: moving_avg,
        });
    }

    enriched
}
