// Device-side change capture for sync v2 (SQLite only).
//
// Every synced table gets AFTER INSERT / AFTER UPDATE triggers that enqueue the
// changed row into `sync_outbox` (coalesced per `(resource, key)`) and record
// its merge metadata in `sync_row_meta`. Triggers are always installed but do
// nothing unless `sync_state.capture_enabled = 1`, so an unlinked install pays
// nothing; they also stay silent while `sync_state.applying = 1`, which is what
// keeps the applier from echoing remote changes back into the outbox.
//
// Deliberately NOT here: reading or applying changes (`modules::sync`).

use sqlx::{Executor, SqlitePool};

use crate::{
    core::{error::AppResult, id::generate_id},
    modules::sync::resources::{Phase, SYNC_RESOURCES},
};

/// Row-meta timestamp of the row being written: its local `updated_at` shifted
/// by the measured clock offset, forced strictly above the previous meta value
/// of the same row so a device whose clock runs behind the cloud can never
/// emit a version older than one it already holds.
fn meta_ms_expr(resource: &str, key_expr: &str, updated_at_expr: &str) -> String {
    format!(
        "MAX(CAST(ROUND((julianday({updated_at_expr}) - 2440587.5) * 86400000.0) AS INTEGER) \
         + (SELECT clock_offset_ms FROM sync_state WHERE id = 1), \
         COALESCE((SELECT updated_at_ms FROM sync_row_meta WHERE resource = '{resource}' AND key = {key_expr}), 0) + 1)"
    )
}

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

/// One trigger pair for `table`, enqueuing `resource` under `key_expr`.
/// `delete_expr` is a SQL boolean deciding whether the change is a tombstone.
fn trigger_sql(
    name: &str,
    table: &str,
    resource: &str,
    key_expr: &str,
    delete_expr: &str,
    version_expr: &str,
    event: &str,
) -> String {
    let ms = meta_ms_expr(resource, key_expr, "NEW.updated_at");
    format!(
        "CREATE TRIGGER IF NOT EXISTS sync_{name}_{suffix} AFTER {event} ON {table}
         WHEN (SELECT capture_enabled = 1 AND applying = 0 FROM sync_state WHERE id = 1)
         BEGIN
           INSERT OR REPLACE INTO sync_outbox (resource, key, op, enqueued_at)
             VALUES ('{resource}', {key_expr},
                     CASE WHEN {delete_expr} THEN 'delete' ELSE 'upsert' END,
                     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
           INSERT OR REPLACE INTO sync_row_meta (resource, key, updated_at_ms, device_id, version)
             VALUES ('{resource}', {key_expr}, {ms},
                     (SELECT device_id FROM sync_state WHERE id = 1), {version_expr});
         END;",
        suffix = if event == "INSERT" { "ins" } else { "upd" },
    )
}

/// `AFTER DELETE` trigger: some tables are removed with a real `DELETE`, which
/// the insert/update triggers cannot see. The deletion is queued as a tombstone
/// (the outbox reader turns a missing row into a `delete` record).
fn delete_trigger_sql(
    name: &str,
    table: &str,
    resource: &str,
    key_expr: &str,
    op: &str,
    version_expr: &str,
) -> String {
    let now_ms = "CAST(ROUND((julianday('now') - 2440587.5) * 86400000.0) AS INTEGER)";
    format!(
        "CREATE TRIGGER IF NOT EXISTS sync_{name}_del AFTER DELETE ON {table}
         WHEN (SELECT capture_enabled = 1 AND applying = 0 FROM sync_state WHERE id = 1)
         BEGIN
           INSERT OR REPLACE INTO sync_outbox (resource, key, op, enqueued_at)
             VALUES ('{resource}', {key_expr}, '{op}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
           INSERT OR REPLACE INTO sync_row_meta (resource, key, updated_at_ms, device_id, version)
             VALUES ('{resource}', {key_expr},
                     MAX({now_ms} + (SELECT clock_offset_ms FROM sync_state WHERE id = 1),
                         COALESCE((SELECT updated_at_ms FROM sync_row_meta WHERE resource = '{resource}' AND key = {key_expr}), 0) + 1),
                     (SELECT device_id FROM sync_state WHERE id = 1), {version_expr});
         END;"
    )
}

/// Installs the capture triggers for every P3a resource plus the
/// `subcategories` -> parent `categories` fold. Idempotent.
pub async fn install_triggers(pool: &SqlitePool) -> AppResult<()> {
    let mut statements: Vec<String> = Vec::new();

    for spec in SYNC_RESOURCES.iter().filter(|s| s.phase == Phase::P3a) {
        let delete_expr = if spec.soft_delete {
            "NEW.deleted_at IS NOT NULL"
        } else {
            "0"
        };
        // product_serials predates the versioning columns.
        let version_expr = if spec.table == "product_serials" {
            "1"
        } else {
            "NEW.version"
        };
        // A write that only touches derived columns (a sale moving
        // `products.stock_quantity`) is not a change of the row: every replica
        // recomputes those from its ledgers. Firing only on the other columns
        // keeps such a write from re-sending the whole row with a fresh
        // timestamp, which would beat a newer edit made elsewhere. Rebuilt on
        // every connect so new columns are covered.
        let update_event = if spec.derived.is_empty() {
            "UPDATE".to_string()
        } else {
            let columns: Vec<String> = sqlx::query_scalar(&format!(
                "SELECT name FROM pragma_table_info('{}')",
                spec.table
            ))
            .fetch_all(pool)
            .await?;
            let watched: Vec<String> = columns
                .into_iter()
                .filter(|c| !spec.derived.contains(&c.as_str()))
                .collect();
            statements.push(format!("DROP TRIGGER IF EXISTS sync_{}_upd", spec.table));
            format!("UPDATE OF {}", watched.join(", "))
        };
        for event in ["INSERT", update_event.as_str()] {
            statements.push(trigger_sql(
                spec.table,
                spec.table,
                spec.name,
                "NEW.key",
                delete_expr,
                version_expr,
                event,
            ));
        }
    }

    for spec in SYNC_RESOURCES.iter().filter(|s| s.phase == Phase::P3a) {
        let version_expr = if spec.table == "product_serials" {
            "1"
        } else {
            "COALESCE(OLD.version, 1) + 1"
        };
        statements.push(delete_trigger_sql(
            spec.table,
            spec.table,
            spec.name,
            "OLD.key",
            "delete",
            version_expr,
        ));
    }
    // Deleting a subcategory changes its parent category (still present).
    statements.push(delete_trigger_sql(
        "subcategories",
        "subcategories",
        "categories",
        "OLD.category_key",
        "upsert",
        "COALESCE((SELECT version FROM categories WHERE key = OLD.category_key), 1)",
    ));

    // A subcategory is not a wire resource: it travels folded into its parent
    // category, so a change to it re-enqueues the parent.
    for event in ["INSERT", "UPDATE"] {
        statements.push(trigger_sql(
            "subcategories",
            "subcategories",
            "categories",
            "NEW.category_key",
            "0",
            "COALESCE((SELECT version FROM categories WHERE key = NEW.category_key), 1)",
            event,
        ));
    }

    for statement in statements {
        pool.execute(statement.as_str()).await?;
    }
    Ok(())
}
