use super::live_credits::live_credits;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::purchases::repository;

/// A supplier's live credits as `(YYYY-MM-DD, amount in cents)`, for the supplier balance.
pub(crate) async fn credits_of_supplier(
    db: &Db,
    supplier_key: &str,
) -> AppResult<Vec<(String, i64)>> {
    let rows = repository::credit::list_credits_for_supplier(db, supplier_key).await?;
    Ok(live_credits(&rows)
        .into_iter()
        .map(|c| {
            (
                c.created_at.to_chrono().format("%Y-%m-%d").to_string(),
                c.amount_cents,
            )
        })
        .collect())
}
