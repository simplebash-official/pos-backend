// Number blocks for linked devices (SQLite). Document numbers (invoice, credit
// note, ticket...) must not collide across offline devices, so a linked device
// draws them from ranges the cloud reserved for it (`sync_number_blocks`,
// filled by the shell's sync agent through `POST /api/sync/blocks`). When a
// block runs dry offline, numbers fall back to `<prefix>D<deviceTag>-<n>`,
// which is unique because cloud numbers never contain the `D<tag>-` infix.
// An unlinked device keeps the plain local counter (see `sequences::service`).

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::{
    core::error::{AppError, AppResult},
    domain::sync_local::StoreBlockRequest,
    modules::sync::state::pool_of,
};

/// Sequence names arrive as aliases (`invoices`, `credit_note`...); blocks are
/// stored and looked up under one canonical name.
pub(crate) fn canonical_name(name: &str) -> String {
    match name.to_lowercase().replace('_', "").as_str() {
        "invoice" | "invoices" => "invoice",
        "creditnote" | "creditnotes" => "creditnote",
        "repair" | "repairs" => "repair",
        "printjob" | "printjobs" => "printjob",
        "purchase" | "purchases" => "purchase",
        "order" | "orders" => "order",
        other => return other.to_string(),
    }
    .to_string()
}

/// A number taken for a document, with the prefix it must be rendered under.
pub(crate) struct DeviceNumber {
    pub prefix: String,
    pub number: i64,
}

/// Stores a block the cloud reserved for this device.
pub async fn store_block(db: &crate::clients::db::Db, req: StoreBlockRequest) -> AppResult<()> {
    crate::core::logging::domain::tracked("sync.block_stored", async move {
        let pool = pool_of(db)?;
        if req.end < req.start || req.padding < 0 {
            return Err(AppError::validation("Invalid number block range"));
        }
        sqlx::query(
            "INSERT OR REPLACE INTO sync_number_blocks (name, prefix, padding, start_seq, end_seq, next_seq, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(canonical_name(&req.name))
        .bind(&req.prefix)
        .bind(req.padding)
        .bind(req.start)
        .bind(req.end)
        .bind(req.start)
        .bind(req.expires_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    })
    .await
}

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
