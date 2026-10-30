use crate::core::error::AppResult;
use crate::core::id::generate_id;

use sqlx::SqlitePool;

/// Creates the single `sync_state` row (with a freshly minted device id) when
/// it does not exist yet, and gives it an outbox epoch. Idempotent; called on
/// every connect.
///
/// The epoch names this database's outbox numbering. Upload batch ids are
/// built from outbox sequence numbers, which restart at 1 when the database is
/// recreated while the device keeps its cloud identity; without the epoch the
/// cloud would replay a stored answer for an old batch with the same id and
/// the new changes would be acknowledged without ever being applied.
pub async fn ensure_state(pool: &SqlitePool) -> AppResult<()> {
    let device_id = generate_id("dev");
    sqlx::query("INSERT OR IGNORE INTO sync_state (id, device_id) VALUES (1, ?)")
        .bind(device_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE sync_state SET outbox_epoch = ? WHERE id = 1 AND outbox_epoch IS NULL")
        .bind(generate_id("ep"))
        .execute(pool)
        .await?;
    Ok(())
}
