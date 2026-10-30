use super::count_by::count_by;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Movements per saved payee key, so the payee combobox can put frequent payees first.
pub async fn count_movements_by_payee(db: &Db) -> AppResult<Vec<(String, i64)>> {
    count_by(db, "payee_key").await
}
