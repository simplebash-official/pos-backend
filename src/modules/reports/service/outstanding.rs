// Business logic for Credit Receivables and Aging reports.

use mongodb::Database;

use crate::{
    core::{error::AppResult, utils::calculate_pagination},
    domain::reports::{OutstandingReceivablesQuery, OutstandingReceivablesResponse},
    modules::reports::repository,
};

/// Retrieves the total uncollected credit, overdue amounts, and paginated unpaid invoices.
pub(crate) async fn get_outstanding_report(
    db: &Database,
    query: OutstandingReceivablesQuery,
) -> AppResult<OutstandingReceivablesResponse> {
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);
    let stats = repository::receivables::aggregate_outstanding_stats(db).await?;
    let (invoices, total) = repository::receivables::list_outstanding_invoices(
        db,
        query.search.as_deref(),
        skip,
        limit,
    )
    .await?;

    let total_pages = total.div_ceil(limit);

    Ok(OutstandingReceivablesResponse {
        total_outstanding_cents: stats.total_outstanding_cents,
        total_credit_invoices_count: stats.total_credit_invoices_count,
        overdue_invoices_count: stats.overdue_invoices_count,
        overdue_amount_cents: stats.overdue_amount_cents,
        invoices,
        total,
        page,
        limit,
        total_pages,
    })
}
