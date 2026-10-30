use crate::domain::reports::CategorySalesRow;

/// Rolls up smaller category slices into a single "Other" entry when there are more than `keep` slices.
#[must_use]
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
