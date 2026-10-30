use super::bps::bps;
use super::range::range;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{
    AnalyticsRangeQuery, CashierPerformanceEntry, CashierPerformanceResponse,
};
use crate::modules::reports::repository;

// ---------------------------------------------------------------------------
// cashier performance
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_cashier_performance(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<CashierPerformanceResponse> {
    let (start, end) = range(&query)?;
    let rows = repository::analytics::aggregate_cashier_performance(db, start, end).await?;
    let refunds = repository::analytics::aggregate_cashier_refunds(db, start, end).await?;

    let total_revenue_cents: i64 = rows.iter().map(|r| r.revenue_cents).sum();
    let total_invoices: u64 = rows.iter().map(|r| r.invoice_count).sum();

    let mut cashiers: Vec<CashierPerformanceEntry> = rows
        .into_iter()
        .map(|r| {
            let (refund_count, refunded_cents) =
                refunds.get(&r.cashier_id).copied().unwrap_or((0, 0));
            let gross_before = r.revenue_cents + r.discount_given_cents;
            CashierPerformanceEntry {
                discount_rate_bps: bps(r.discount_given_cents, gross_before),
                avg_basket_cents: if r.invoice_count == 0 {
                    0
                } else {
                    r.revenue_cents / r.invoice_count as i64
                },
                cashier_id: r.cashier_id,
                cashier_name: r.cashier_name,
                invoice_count: r.invoice_count,
                revenue_cents: r.revenue_cents,
                retail_revenue_cents: r.retail_revenue_cents,
                items_sold: r.items_sold,
                discount_given_cents: r.discount_given_cents,
                credit_invoice_count: r.credit_invoice_count,
                refund_count,
                refunded_cents,
                gross_profit_cents: r.gross_profit_cents,
            }
        })
        .collect();
    cashiers.sort_by_key(|c| std::cmp::Reverse(c.revenue_cents));

    Ok(CashierPerformanceResponse {
        cashiers,
        total_revenue_cents,
        total_invoices,
    })
}
