use super::super::dates;

use super::bps::bps;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::reports::{
    AnalyticsKpiDeltas, AnalyticsKpis, AnalyticsRangeQuery, AnalyticsSummaryResponse,
};
use crate::modules::reports::repository;
use chrono::{DateTime, Utc};

/// Percentage change `(cur - prev) / prev` in bps, `None` when `prev` is 0.
fn delta_bps(cur: i64, prev: i64) -> Option<i64> {
    if prev == 0 {
        None
    } else {
        Some((((cur - prev) as i128 * 10_000) / prev as i128) as i64)
    }
}

// ---------------------------------------------------------------------------
// summary
// ---------------------------------------------------------------------------
pub(crate) async fn get_analytics_summary(
    db: &Db,
    query: AnalyticsRangeQuery,
) -> AppResult<AnalyticsSummaryResponse> {
    let (start, end) = dates::parse_date_range_tz(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;

    let (current, previous, deltas) = if query.compare_previous.unwrap_or(false) {
        let span = end - start;
        let prev_start = start - span;
        let (cur, prev) = tokio::try_join!(
            compute_kpis(db, start, end),
            compute_kpis(db, prev_start, start),
        )?;
        let deltas = AnalyticsKpiDeltas {
            total_revenue_bps: delta_bps(cur.total_revenue_cents, prev.total_revenue_cents),
            gross_profit_bps: delta_bps(cur.gross_profit_cents, prev.gross_profit_cents),
            net_profit_bps: delta_bps(cur.net_profit_cents, prev.net_profit_cents),
            invoice_count_bps: delta_bps(cur.invoice_count as i64, prev.invoice_count as i64),
            avg_basket_bps: delta_bps(cur.avg_basket_cents, prev.avg_basket_cents),
            gross_margin_delta_bps: cur.gross_margin_bps - prev.gross_margin_bps,
        };
        (cur, Some(prev), Some(deltas))
    } else {
        let cur = compute_kpis(db, start, end).await?;
        (cur, None, None)
    };

    Ok(AnalyticsSummaryResponse {
        period_start: start,
        period_end: end,
        current,
        previous,
        deltas,
    })
}

/// The full KPI block for one `[start, end)` window.
async fn compute_kpis(
    db: &Db,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> AppResult<AnalyticsKpis> {
    let (invoice, commissions, refunds) = tokio::try_join!(
        repository::analytics::aggregate_invoice_kpis(db, start, end),
        repository::commissions::aggregate_employee_commissions(db, start, end, None),
        repository::refunds::aggregate_refund_totals(db, start, end),
    )?;

    let mut service_material_cost_cents = 0;
    let mut commission_payouts_cents = 0;
    for emp in commissions.values() {
        service_material_cost_cents += emp.estimated_cost_cents;
        commission_payouts_cents += emp.earned_commission_cents;
    }

    let total_revenue_cents = invoice.net_sales_cents;
    let total_cogs_cents = invoice.retail_cogs_cents + service_material_cost_cents;
    let gross_profit_cents = total_revenue_cents - total_cogs_cents;
    let net_profit_cents = gross_profit_cents - commission_payouts_cents - refunds.net_refund_cents;

    Ok(AnalyticsKpis {
        total_revenue_cents,
        retail_revenue_cents: invoice.retail_revenue_cents,
        repair_revenue_cents: invoice.repair_revenue_cents,
        print_revenue_cents: invoice.print_revenue_cents,
        invoice_count: invoice.invoice_count,
        items_sold: invoice.retail_items_sold,
        avg_basket_cents: if invoice.invoice_count == 0 {
            0
        } else {
            total_revenue_cents / invoice.invoice_count as i64
        },
        discount_cents: invoice.discount_cents,
        discount_rate_bps: bps(invoice.discount_cents, invoice.gross_sales_cents),
        retail_cogs_cents: invoice.retail_cogs_cents,
        service_material_cost_cents,
        gross_profit_cents,
        gross_margin_bps: bps(gross_profit_cents, total_revenue_cents),
        commission_payouts_cents,
        refunds_cents: refunds.net_refund_cents,
        refund_rate_bps: bps(refunds.net_refund_cents, total_revenue_cents),
        net_profit_cents,
        cogs_coverage_bps: bps(
            invoice.retail_revenue_with_cost_cents,
            invoice.retail_revenue_cents,
        ),
    })
}
