use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::customers::CustomerStats;

use crate::modules::customers::repository;

/// Computes the KPI cards for the Customers screen.
pub async fn get_customer_stats(db: &Db) -> AppResult<CustomerStats> {
    let agg = repository::aggregate_stats(db).await?;
    Ok(CustomerStats {
        total_customers: agg.total_customers,
        total_balance_due_cents: agg.total_balance_due_cents,
        active_debtors_count: agg.active_debtors_count,
    })
}
