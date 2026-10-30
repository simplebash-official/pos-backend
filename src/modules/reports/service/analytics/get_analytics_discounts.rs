use super::bps::bps;
use super::range::range;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{
    AnalyticsRangeQuery, DiscountAnalyticsResponse, DiscountCashierRow, DiscountProductRow,
    DiscountTypeRow,
};
use crate::modules::reports::repository;

// ---------------------------------------------------------------------------
// discounts
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_discounts(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<DiscountAnalyticsResponse> {
    let (start, end) = range(&query)?;
    let agg = repository::analytics::aggregate_discounts(db, start, end).await?;

    let total_discount_cents = agg.order_level_discount_cents + agg.line_level_discount_cents;

    Ok(DiscountAnalyticsResponse {
        period_start: start,
        period_end: end,
        total_discount_cents,
        gross_before_discount_cents: agg.gross_before_discount_cents,
        discount_rate_bps: bps(total_discount_cents, agg.gross_before_discount_cents),
        invoices_with_discount: agg.invoices_with_discount,
        invoice_count: agg.invoice_count,
        order_level_discount_cents: agg.order_level_discount_cents,
        line_level_discount_cents: agg.line_level_discount_cents,
        by_type: agg
            .by_type
            .into_iter()
            .map(
                |(discount_type, invoice_count, discount_cents)| DiscountTypeRow {
                    discount_type,
                    invoice_count,
                    discount_cents,
                },
            )
            .collect(),
        by_cashier: {
            let mut v: Vec<DiscountCashierRow> = agg
                .by_cashier
                .into_iter()
                .map(
                    |(cashier_id, cashier_name, discount_cents, revenue_cents)| {
                        DiscountCashierRow {
                            discount_rate_bps: bps(discount_cents, revenue_cents + discount_cents),
                            cashier_id,
                            cashier_name,
                            discount_cents,
                            revenue_cents,
                        }
                    },
                )
                .collect();
            v.sort_by_key(|r| std::cmp::Reverse(r.discount_cents));
            v
        },
        top_discounted_products: agg
            .top_products
            .into_iter()
            .map(
                |(product_key, name, discount_cents, units_sold)| DiscountProductRow {
                    product_key,
                    name,
                    discount_cents,
                    units_sold,
                },
            )
            .collect(),
    })
}
