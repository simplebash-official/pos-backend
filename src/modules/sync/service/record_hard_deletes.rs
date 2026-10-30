use crate::clients::db::Db;
use crate::core::error::AppResult;

/// Tells cloud sync that `keys` of `table` were removed outright, so devices
/// remove them too (a hard delete leaves the change stream nothing to read).
/// Call it right after the Mongo delete; a no-op everywhere but the cloud.
pub(crate) async fn record_hard_deletes(db: &Db, table: &str, keys: &[String]) -> AppResult<()> {
    super::super::cloud_store::record_hard_deletes(db, table, keys).await
}
