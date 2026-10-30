use super::count_by::count_by;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Movements per category key, for ranking the picker by this shop's real usage.
pub async fn count_movements_by_category(db: &Db) -> AppResult<Vec<(String, i64)>> {
    count_by(db, "category_key").await
}
