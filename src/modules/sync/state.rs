// Device-local sync bookkeeping (SQLite): the single `sync_state` row, enabling
// capture (opening-balance ledger migration + outbox seeding), acknowledging
// pushed outbox rows and the reviewable conflict list. Nothing here talks to
// the cloud - the desktop shell's sync agent drives it through the local
// `/api/sync/*` routes.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::{
    clients::{db::Db, sqlite::generate_id_hex},
    core::error::{AppError, AppResult},
    domain::sync_local::{
        ConflictItem, EnableRequest, EnableResponse, NumberBlockInfo, SeedResponse,
        SyncStateResponse, UpdateSyncStateRequest,
    },
    modules::sync::resources::{Phase, SYNC_RESOURCES},
};

/// The SQLite pool, or a clear error when the local sync API is reached on a
/// MongoDB (cloud) deployment, where it does not exist.
pub(crate) fn pool_of(db: &Db) -> AppResult<&SqlitePool> {
    db.as_sqlite().ok_or_else(|| {
        AppError::not_found("The local sync API is only available on desktop (SQLite) installs")
    })
}

pub async fn get_state(db: &Db) -> AppResult<SyncStateResponse> {
    let pool = pool_of(db)?;
    let row = sqlx::query(
        "SELECT device_id, tenant_id, linked, capture_enabled, cloud_cursor, last_pushed_outbox_seq, \
         clock_offset_ms, bootstrap_active FROM sync_state WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    let pending_out: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox")
        .fetch_one(pool)
        .await?;
    let conflicts_open: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_conflicts WHERE resolved_at IS NULL")
            .fetch_one(pool)
            .await?;
    let local_has_data: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM invoices)")
        .fetch_one(pool)
        .await?;
    let block_rows = sqlx::query(
        "SELECT name, \
                SUM(CASE WHEN end_seq >= next_seq THEN end_seq - next_seq + 1 ELSE 0 END) AS remaining, \
                MAX(end_seq - start_seq + 1) AS block_size \
         FROM sync_number_blocks WHERE expires_at > ? GROUP BY name ORDER BY name",
    )
    .bind(Utc::now().to_rfc3339())
    .fetch_all(pool)
    .await?;
    let number_blocks = block_rows
        .into_iter()
        .map(|r| NumberBlockInfo {
            name: r.get("name"),
            remaining: r.get("remaining"),
            block_size: r.get("block_size"),
        })
        .collect();
    Ok(SyncStateResponse {
        device_id: row.get("device_id"),
        tenant_id: row.get("tenant_id"),
        linked: row.get::<i64, _>("linked") != 0,
        capture_enabled: row.get::<i64, _>("capture_enabled") != 0,
        cloud_cursor: row.get("cloud_cursor"),
        last_pushed_outbox_seq: row.get("last_pushed_outbox_seq"),
        clock_offset_ms: row.get("clock_offset_ms"),
        bootstrap_active: row.get::<i64, _>("bootstrap_active") != 0,
        pending_out,
        conflicts_open,
        local_has_data,
        number_blocks,
    })
}

pub async fn update_state(db: &Db, req: UpdateSyncStateRequest) -> AppResult<SyncStateResponse> {
    crate::core::logging::domain::tracked("sync.state_updated", async move {
        let pool = pool_of(db)?;
        let mut tx = pool.begin().await?;
        if let Some(offset) = req.clock_offset_ms {
            sqlx::query("UPDATE sync_state SET clock_offset_ms = ? WHERE id = 1")
                .bind(offset)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(linked) = req.linked {
            sqlx::query("UPDATE sync_state SET linked = ? WHERE id = 1")
                .bind(linked as i64)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(tenant_id) = &req.tenant_id {
            sqlx::query("UPDATE sync_state SET tenant_id = ? WHERE id = 1")
                .bind(tenant_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(cursor) = req.cloud_cursor {
            sqlx::query("UPDATE sync_state SET cloud_cursor = ? WHERE id = 1")
                .bind(cursor)
                .execute(&mut *tx)
                .await?;
        }
        if req.bootstrap_reset == Some(true) {
            sqlx::query("UPDATE sync_state SET bootstrap_active = 0 WHERE id = 1")
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        get_state(db).await
    })
    .await
}

/// Turns capture on: adopts the cloud-issued device identity, makes stock a pure
/// ledger, flips `capture_enabled` and `linked`, then enqueues every existing
/// row so the first push uploads the whole shop.
pub async fn enable(db: &Db, req: EnableRequest) -> AppResult<EnableResponse> {
    crate::core::logging::domain::tracked("sync.enabled", async move {
        let pool = pool_of(db)?;
        let opening_balances_created = migrate_opening_balances(pool).await?;
        if let Some(device_id) = req.device_id.as_deref().filter(|d| !d.is_empty()) {
            sqlx::query("UPDATE sync_state SET device_id = ? WHERE id = 1")
                .bind(device_id)
                .execute(pool)
                .await?;
        }
        if let Some(tenant_id) = req.tenant_id.as_deref().filter(|t| !t.is_empty()) {
            sqlx::query("UPDATE sync_state SET tenant_id = ? WHERE id = 1")
                .bind(tenant_id)
                .execute(pool)
                .await?;
        }
        sqlx::query("UPDATE sync_state SET capture_enabled = 1, linked = 1 WHERE id = 1")
            .execute(pool)
            .await?;
        let enqueued = seed_outbox(db).await?.enqueued;
        let device_id: String = sqlx::query_scalar("SELECT device_id FROM sync_state WHERE id = 1")
            .fetch_one(pool)
            .await?;
        Ok(EnableResponse {
            device_id,
            opening_balances_created,
            enqueued,
        })
    })
    .await
}

/// Enqueues every row of every synced table (and records its merge metadata)
/// - the initial upload of a device that becomes a tenant's first device.
pub async fn seed_outbox(db: &Db) -> AppResult<SeedResponse> {
    let pool = pool_of(db)?;
    let mut tx = pool.begin().await?;
    let mut enqueued = 0i64;
    for spec in SYNC_RESOURCES.iter().filter(|s| s.phase == Phase::P3a) {
        let op_expr = if spec.soft_delete {
            "CASE WHEN deleted_at IS NOT NULL THEN 'delete' ELSE 'upsert' END"
        } else {
            "'upsert'"
        };
        let version_expr = if spec.table == "product_serials" {
            "1"
        } else {
            "version"
        };
        let inserted = sqlx::query(&format!(
            "INSERT OR REPLACE INTO sync_outbox (resource, key, op, enqueued_at) \
             SELECT '{name}', key, {op_expr}, strftime('%Y-%m-%dT%H:%M:%fZ', 'now') FROM {table}",
            name = spec.name,
            table = spec.table,
        ))
        .execute(&mut *tx)
        .await?
        .rows_affected();
        enqueued += inserted as i64;
        sqlx::query(&format!(
            "INSERT OR REPLACE INTO sync_row_meta (resource, key, updated_at_ms, device_id, version) \
             SELECT '{name}', key, \
                    CAST(ROUND((julianday(updated_at) - 2440587.5) * 86400000.0) AS INTEGER) \
                      + (SELECT clock_offset_ms FROM sync_state WHERE id = 1), \
                    (SELECT device_id FROM sync_state WHERE id = 1), {version_expr} FROM {table}",
            name = spec.name,
            table = spec.table,
        ))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(SeedResponse { enqueued })
}

/// Drops outbox rows the cloud has accepted (`seq <= up_to`). Rows enqueued
/// while a push was in flight have higher sequence numbers and are kept.
pub async fn ack_outbox(db: &Db, up_to_seq: i64) -> AppResult<i64> {
    crate::core::logging::domain::tracked("sync.outbox_acked", async move {
        let pool = pool_of(db)?;
        let mut tx = pool.begin().await?;
        let removed = sqlx::query("DELETE FROM sync_outbox WHERE seq <= ?")
            .bind(up_to_seq)
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;
        sqlx::query(
            "UPDATE sync_state SET last_pushed_outbox_seq = MAX(last_pushed_outbox_seq, ?) WHERE id = 1",
        )
        .bind(up_to_seq)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(removed)
    })
    .await
}

/// Gives every product whose `stock_quantity` is not explained by its
/// movements one deterministic `opening_balance` movement for the difference.
/// Idempotent: the movement key is derived from the product key, and once the
/// ledger matches there is nothing left to create.
pub async fn migrate_opening_balances(pool: &SqlitePool) -> AppResult<i64> {
    let rows = sqlx::query(
        "SELECT p.key AS key, p.id AS id, p.stock_quantity AS stock, \
                COALESCE((SELECT SUM(m.quantity_delta) FROM stock_movements m \
                          WHERE m.product_id = p.id AND m.deleted_at IS NULL), 0) AS ledger \
         FROM products p WHERE p.deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let now = Utc::now().to_rfc3339();
    let mut created = 0i64;
    for row in rows {
        let stock: i64 = row.get("stock");
        let ledger: i64 = row.get("ledger");
        if stock == ledger {
            continue;
        }
        let product_key: String = row.get("key");
        let done = sqlx::query(
            "INSERT OR IGNORE INTO stock_movements \
             (key, id, product_id, quantity_delta, movement_type, reference_id, note, version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'opening_balance', ?, 'Opening balance (ledger migration)', 1, ?, ?)",
        )
        .bind(format!("sm_open_{product_key}"))
        .bind(generate_id_hex())
        .bind(row.get::<String, _>("id"))
        .bind(stock - ledger)
        .bind(&product_key)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        created += done.rows_affected() as i64;
    }
    Ok(created)
}

pub async fn list_conflicts(db: &Db) -> AppResult<Vec<ConflictItem>> {
    let pool = pool_of(db)?;
    let rows = sqlx::query(
        "SELECT key, kind, resource, entity_key, detail, detected_at, resolved_at, resolution \
         FROM sync_conflicts WHERE resolved_at IS NULL ORDER BY detected_at DESC, key",
    )
    .fetch_all(pool)
    .await?;
    let parse = |s: String| -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    };
    Ok(rows
        .into_iter()
        .map(|row| ConflictItem {
            key: row.get("key"),
            kind: row.get("kind"),
            resource: row.get("resource"),
            entity_key: row.get("entity_key"),
            detail: serde_json::from_str(&row.get::<String, _>("detail")).unwrap_or_default(),
            detected_at: parse(row.get("detected_at")),
            resolved_at: row.get::<Option<String>, _>("resolved_at").map(parse),
            resolution: row.get("resolution"),
        })
        .collect())
}

pub async fn resolve_conflict(db: &Db, key: &str, resolution: &str) -> AppResult<()> {
    crate::core::logging::domain::tracked("sync.conflict_resolved", async move {
        let pool = pool_of(db)?;
        let done = sqlx::query(
            "UPDATE sync_conflicts SET resolved_at = ?, resolution = ? WHERE key = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(resolution)
        .bind(key)
        .execute(pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(AppError::not_found("Conflict not found or already resolved"));
        }
        Ok(())
    })
    .await
}
