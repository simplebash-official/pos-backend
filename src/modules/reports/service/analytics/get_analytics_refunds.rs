use super::bps::bps;
use super::range::range;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{
    AnalyticsRangeQuery, RefundAnalyticsResponse, RefundMethodRow, RefundProductRow,
    RefundReasonRow,
};
use crate::modules::reports::repository;

// ---------------------------------------------------------------------------
// refunds
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_refunds(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<RefundAnalyticsResponse> {
    let (start, end) = range(&query)?;
    let agg = repository::analytics::aggregate_refund_detail(db, start, end).await?;
    let sales = repository::analytics::aggregate_invoice_kpis(db, start, end).await?;

    Ok(RefundAnalyticsResponse {
        period_start: start,
        period_end: end,
        credit_note_count: agg.credit_note_count,
        net_refund_cents: agg.net_refund_cents,
        refund_cash_cents: agg.refund_cash_cents,
        balance_reduction_cents: agg.balance_reduction_cents,
        refund_rate_bps: bps(agg.net_refund_cents, sales.net_sales_cents),
        exchange_count: agg.exchange_count,
        no_receipt_count: agg.no_receipt_count,
        manager_override_count: agg.manager_override_count,
        by_reason: agg
            .by_reason
            .into_iter()
            .map(
                |(reason, credit_note_item_count, amount_cents)| RefundReasonRow {
                    reason,
                    credit_note_item_count,
                    amount_cents,
                },
            )
            .collect(),
        by_method: agg
            .by_method
            .into_iter()
            .map(|(method, amount_cents)| RefundMethodRow {
                method,
                amount_cents,
            })
            .collect(),
        top_returned_products: agg
            .top_products
            .into_iter()
            .map(
                |(product_key, name, quantity, amount_cents)| RefundProductRow {
                    product_key,
                    name,
                    quantity,
                    amount_cents,
                },
            )
            .collect(),
    })
}
