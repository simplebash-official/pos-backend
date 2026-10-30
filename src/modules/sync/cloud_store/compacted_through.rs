use super::meta_i64::meta_i64;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Changes at or below this seq were compacted away; a puller behind it must
/// bootstrap from a snapshot.
pub(crate) async fn compacted_through(db: &Db) -> AppResult<i64> {
    meta_i64(db, "compacted_through_seq").await
}
