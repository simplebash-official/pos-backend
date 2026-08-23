// Business logic for the Reports & Profit Intelligence dashboard overview.

use mongodb::Database;

use super::dates;
use crate::{
    core::error::AppResult,
    domain::reports::{ReportDateRangeQuery, ReportsDashboardResponse},
    modules::reports::repository,
};

/// Computes unified executive KPI metrics across sales, revenue streams, commissions,
/// profit calculations, and credit receivables.
pub(crate) async fn get_dashboard_overview(
    db: &Database,
    query: ReportDateRangeQuery,
) -> AppResult<ReportsDashboardResponse> {
    let (start, end) = dates::parse_date_range(
        query.preset.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
    )?;

    let sales = repository::sales::aggregate_sales_summary(db, start, end).await?;
    let commissions =
        repository::commissions::aggregate_employee_commissions(db, start, end, None).await?;
    let receivables = repository::receivables::aggregate_outstanding_stats(db).await?;

    let mut estimated_cogs_cents = 0;
    for emp in &commissions.employees {
        estimated_cogs_cents += emp.estimated_cost_cents;
    }

    let estimated_gross_profit_cents = (sales.total_sales_cents - estimated_cogs_cents).max(0);
    let total_commissions_cents = commissions.total_commissions_cents;
    let net_shop_profit_cents = (estimated_gross_profit_cents - total_commissions_cents).max(0);

    Ok(ReportsDashboardResponse {
        total_revenue_cents: sales.total_sales_cents,
        retail_revenue_cents: sales.retail_revenue_cents,
        repair_revenue_cents: sales.repair_revenue_cents,
        print_revenue_cents: sales.print_revenue_cents,
        total_invoices_count: sales.total_invoices,
        total_discounts_cents: sales.discount_cents,
        estimated_cogs_cents,
        estimated_gross_profit_cents,
        total_commissions_cents,
        net_shop_profit_cents,
        outstanding_credit_cents: receivables.total_outstanding_cents,
        period_start: start,
        period_end: end,
    })
}
