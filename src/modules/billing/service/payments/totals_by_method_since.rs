use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::repository;

/// What customers paid and were refunded by method from `since` (`YYYY-MM-DD`) on, as `(method, received, refunded)`.
pub(crate) async fn totals_by_method_since(
    db: &Db,
    since: &str,
) -> AppResult<Vec<(String, i64, i64)>> {
    repository::payment_totals_by_method(db, since).await
}
