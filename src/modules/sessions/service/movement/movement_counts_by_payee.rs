use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::repository::movements;

/// Movements per saved payee key, for ranking the payee list.
pub(crate) async fn movement_counts_by_payee(db: &Db) -> AppResult<Vec<(String, i64)>> {
    movements::count_movements_by_payee(db).await
}
