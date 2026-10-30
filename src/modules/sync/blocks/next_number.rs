use super::canonical_name::canonical_name;
use super::device_number::DeviceNumber;
use crate::core::error::AppResult;

use chrono::Utc;
use sqlx::{Row, SqlitePool};

/// The next number for `name` on a LINKED device, or `None` for an unlinked one
/// (the caller then uses the ordinary local counter).
pub(crate) async fn next_number(
    pool: &SqlitePool,
    name: &str,
    default_prefix: &str,
    padding: usize,
) -> AppResult<Option<DeviceNumber>> {
    let linked: i64 = sqlx::query_scalar("SELECT linked FROM sync_state WHERE id = 1")
        .fetch_optional(pool)
        .await?
        .unwrap_or(0);
    if linked == 0 {
        return Ok(None);
    }
    let name = canonical_name(name);

    // One statement takes the number, so concurrent sales never share one.
    let taken = sqlx::query(
        "UPDATE sync_number_blocks SET next_seq = next_seq + 1 \
         WHERE rowid = (SELECT rowid FROM sync_number_blocks \
                        WHERE name = ? AND expires_at > ? AND next_seq <= end_seq \
                        ORDER BY start_seq LIMIT 1) \
         RETURNING prefix, next_seq - 1 AS number",
    )
    .bind(&name)
    .bind(Utc::now().to_rfc3339())
    .fetch_optional(pool)
    .await?;
    if let Some(row) = taken {
        return Ok(Some(DeviceNumber {
            prefix: row.get("prefix"),
            number: row.get("number"),
        }));
    }

    // Block exhausted while offline: a per-device fallback series.
    let device_id: String = sqlx::query_scalar("SELECT device_id FROM sync_state WHERE id = 1")
        .fetch_one(pool)
        .await?;
    let tag: String = device_id
        .strip_prefix("dev_")
        .unwrap_or(&device_id)
        .chars()
        .take(4)
        .collect();
    let number: i64 = sqlx::query_scalar(
        "INSERT INTO sequence_counters (name, prefix, padding, next_val, updated_at) \
         VALUES (?, ?, ?, 1, ?) \
         ON CONFLICT(name) DO UPDATE SET next_val = next_val + 1, updated_at = excluded.updated_at \
         RETURNING next_val",
    )
    .bind(format!("{name}:fallback"))
    .bind(default_prefix)
    .bind(padding as i64)
    .bind(Utc::now().to_rfc3339())
    .fetch_one(pool)
    .await?;
    Ok(Some(DeviceNumber {
        prefix: format!("{default_prefix}D{tag}-"),
        number,
    }))
}
