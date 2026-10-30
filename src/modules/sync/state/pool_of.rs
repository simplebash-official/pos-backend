use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use sqlx::SqlitePool;

/// The SQLite pool, or a clear error when the local sync API is reached on a
/// MongoDB (cloud) deployment, where it does not exist.
pub(crate) fn pool_of(db: &Db) -> AppResult<&SqlitePool> {
    db.as_sqlite().ok_or_else(|| {
        AppError::not_found("The local sync API is only available on desktop (SQLite) installs")
    })
}
