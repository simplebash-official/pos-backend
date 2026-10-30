use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::sessions::repository::movements;

/// Movements per category key (including none merged), for ranking the category picker.
pub(crate) async fn movement_counts_by_category(db: &Db) -> AppResult<Vec<(String, i64)>> {
    movements::count_movements_by_category(db).await
}
