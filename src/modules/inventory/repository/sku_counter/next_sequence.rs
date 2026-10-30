use super::bump::bump;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Atomically increments and returns the next sequence number for `code`,
/// creating the counter starting at 1 if it doesn't exist yet.
pub(crate) async fn next_sequence(db: &Db, code: &str) -> AppResult<i64> {
    match db {
        Db::Mongo(db) => bump(db, code, 1).await,
        Db::Sqlite(pool) => {
            let seq: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO sku_counters (prefix, seq) VALUES (?, 1)
                ON CONFLICT(prefix) DO UPDATE SET seq = seq + 1
                RETURNING seq
                "#,
            )
            .bind(code)
            .fetch_one(pool)
            .await?;

            Ok(seq)
        }
    }
}
