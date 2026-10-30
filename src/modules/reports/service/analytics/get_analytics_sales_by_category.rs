use super::super::dates;

use super::bps::bps;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{CategorySalesRow, SalesByCategoryQuery, SalesByCategoryResponse};
use crate::modules::reports::repository;

// ---------------------------------------------------------------------------
// sales by category
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_sales_by_category(
    db: &Db,
    query: SalesByCategoryQuery,
) -> AppResult<SalesByCategoryResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;
    let by_subcategory = query.group_by.as_deref() == Some("subcategory");

    let raw =
        repository::analytics::aggregate_sales_by_category(db, start, end, by_subcategory).await?;

    let mut rows = Vec::new();
    let mut uncategorised = CategorySalesRow {
        category_name: "Uncategorised".to_string(),
        ..Default::default()
    };
    let mut total_revenue_cents = 0;
    let mut total_gross_profit_cents = 0;

    for r in raw {
        let gross_profit = r.revenue_cents - r.cogs_cents;
        total_revenue_cents += r.revenue_cents;
        total_gross_profit_cents += gross_profit;
        let row = CategorySalesRow {
            category_key: r.category_key.clone(),
            category_name: r.category_name,
            subcategory_key: r.subcategory_key,
            subcategory_name: r.subcategory_name,
            units_sold: r.units_sold,
            revenue_cents: r.revenue_cents,
            discount_cents: r.discount_cents,
            cogs_cents: r.cogs_cents,
            gross_profit_cents: gross_profit,
            gross_margin_bps: bps(gross_profit, r.revenue_cents),
        };
        if r.category_key.is_none() {
            uncategorised.units_sold += row.units_sold;
            uncategorised.revenue_cents += row.revenue_cents;
            uncategorised.discount_cents += row.discount_cents;
            uncategorised.cogs_cents += row.cogs_cents;
            uncategorised.gross_profit_cents += row.gross_profit_cents;
        } else {
            rows.push(row);
        }
    }
    uncategorised.gross_margin_bps = bps(
        uncategorised.gross_profit_cents,
        uncategorised.revenue_cents,
    );
    rows.sort_by_key(|r| std::cmp::Reverse(r.revenue_cents));

    Ok(SalesByCategoryResponse {
        rows,
        uncategorised,
        total_revenue_cents,
        total_gross_profit_cents,
    })
}
