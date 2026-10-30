use crate::domain::reports::PatternCell;

/// Identifies peak sales hour and peak day from 168-cell sales pattern heatmap.
#[must_use]
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
