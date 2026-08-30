// High-speed Chart & Analytics Vectorizer.
// Precomputes cumulative curves, moving averages, top-slice normalization,
// and trend metrics in Rust in microseconds.

use crate::domain::reports::{CategorySalesRow, PatternCell, TimeSeriesPoint};

/// Enriched time series point with cumulative metrics and moving averages.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedTimeSeriesPoint {
    #[serde(flatten)]
    pub point: TimeSeriesPoint,
    pub cumulative_revenue_cents: i64,
    pub cumulative_profit_cents: i64,
    pub moving_avg_revenue_cents: Option<i64>,
}

/// Computes cumulative curves and a simple moving average (window = 7) over a time series.
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

/// Rolls up smaller category slices into a single "Other" entry when there are more than `keep` slices.
pub fn top_categories_with_other(
    mut rows: Vec<CategorySalesRow>,
    keep: usize,
) -> Vec<CategorySalesRow> {
    if rows.len() <= keep + 1 {
        return rows;
    }

    rows.sort_by_key(|b| std::cmp::Reverse(b.revenue_cents));

    if rows.len() <= keep {
        return rows;
    }

    let top = rows[..keep].to_vec();
    let remaining = &rows[keep..];

    let mut other_units = 0;
    let mut other_rev = 0;
    let mut other_disc = 0;
    let mut other_cogs = 0;
    let mut other_profit = 0;

    for r in remaining {
        other_units += r.units_sold;
        other_rev += r.revenue_cents;
        other_disc += r.discount_cents;
        other_cogs += r.cogs_cents;
        other_profit += r.gross_profit_cents;
    }

    let other_margin_bps = if other_rev > 0 {
        ((other_profit as i128 * 10_000) / other_rev as i128) as i64
    } else {
        0
    };

    let other_row = CategorySalesRow {
        category_key: None,
        category_name: "Other Categories".to_string(),
        subcategory_key: None,
        subcategory_name: None,
        units_sold: other_units,
        revenue_cents: other_rev,
        discount_cents: other_disc,
        cogs_cents: other_cogs,
        gross_profit_cents: other_profit,
        gross_margin_bps: other_margin_bps,
    };

    let mut result = top;
    result.push(other_row);
    result
}

/// Identifies peak sales hour and peak day from 168-cell sales pattern heatmap.
pub fn analyze_sales_patterns(cells: &[PatternCell]) -> (Option<(u8, u8)>, i64) {
    let mut max_rev = 0;
    let mut peak_cell = None;

    for c in cells {
        if c.revenue_cents > max_rev {
            max_rev = c.revenue_cents;
            peak_cell = Some((c.weekday, c.hour));
        }
    }

    (peak_cell, max_rev)
}
