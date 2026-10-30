use crate::core::error::AppResult;

use chrono::Utc;
use sqlx::SqlitePool;

/// The next RAW sequence number from a stored block under `name`, or `None`
/// if the device is unlinked or holds no live block. Unlike `next_number`,
/// this never falls back to a device-tagged series itself — an EAN-13
/// barcode is a fixed-format 13-digit numeric string, so injecting text like
/// `next_number`'s `D<tag>-` prefix would corrupt it. The caller
/// (`barcode::service::generator::generate`) applies its own numeric-safe
/// device tag when this returns `None`.
pub(crate) async fn next_raw_number(pool: &SqlitePool, name: &str) -> AppResult<Option<i64>> {
    let linked: i64 = sqlx::query_scalar("SELECT linked FROM sync_state WHERE id = 1")
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);
    if linked == 0 {
        return Ok(None);
    }

    let taken: Option<i64> = sqlx::query_scalar(
        "UPDATE sync_number_blocks SET next_seq = next_seq + 1 \
         WHERE rowid = (SELECT rowid FROM sync_number_blocks \
                        WHERE name = ? AND expires_at > ? AND next_seq <= end_seq \
                        ORDER BY start_seq LIMIT 1) \
         RETURNING next_seq - 1",
    )
    .bind(name)
    .bind(Utc::now().to_rfc3339())
    .fetch_optional(pool)
    .await?;
    Ok(taken)
}
