use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::modules::reports::repository;
use chrono::Utc;

/// What was sold, what the sold items cost and what was refunded between two moments, as
/// `(sales_cents, cost_of_goods_cents, refunds_cents)`, for the profit and loss report. Unlike the monthly
/// profit above, nothing is floored at zero: a loss stays a loss.
pub(crate) async fn sales_cost_and_refunds(
    db: &Db,
    start: chrono::DateTime<Utc>,
    end: chrono::DateTime<Utc>,
) -> AppResult<(i64, i64, i64)> {
    let sales = repository::sales::aggregate_sales_summary(db, start, end).await?;
    let commissions =
        repository::commissions::aggregate_employee_commissions(db, start, end, None).await?;
    let refunds = repository::refunds::aggregate_refund_totals(db, start, end)
        .await?
        .net_refund_cents;
    let cost = commissions.values().map(|e| e.estimated_cost_cents).sum();
    Ok((sales.total_sales_cents, cost, refunds))
}
