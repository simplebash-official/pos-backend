use super::bump::bump;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Atomically reserves a contiguous block of `block_size` sequence numbers
/// for `code`, drawn from the exact same counter row `next_sequence`
/// increments one at a time — a block a linked device pre-fetches and a SKU
/// generated directly by `generate_sku` (unlinked, or online with no block
/// held) can therefore never land on the same number.
pub(crate) async fn reserve_block(db: &Db, code: &str, block_size: i64) -> AppResult<(i64, i64)> {
    match db {
        Db::Mongo(db) => {
            let seq = bump(db, code, block_size).await?;
            Ok((seq - block_size + 1, seq))
        }
        Db::Sqlite(pool) => {
            let seq: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO sku_counters (prefix, seq) VALUES (?, ?)
                ON CONFLICT(prefix) DO UPDATE SET seq = seq + ?
                RETURNING seq
                "#,
            )
            .bind(code)
            .bind(block_size)
            .bind(block_size)
            .fetch_one(pool)
            .await?;
            Ok((seq - block_size + 1, seq))
        }
    }
}
