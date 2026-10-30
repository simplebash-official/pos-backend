use super::meta_i64::meta_i64;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Highest sequence number allocated so far for the tenant.
pub(crate) async fn current_seq(db: &Db) -> AppResult<i64> {
    meta_i64(db, "last_seq").await
}
