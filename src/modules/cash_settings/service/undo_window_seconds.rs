use super::document::document;
use crate::clients::db::Db;
use crate::core::error::AppResult;

/// Seconds a cashier can undo their own movement.
pub(crate) async fn undo_window_seconds(db: &Db) -> AppResult<i64> {
    Ok(document(db).await?.undo_window_seconds)
}
