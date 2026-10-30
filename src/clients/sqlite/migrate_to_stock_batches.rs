use super::table_columns::table_columns;

use sqlx::{Executor, SqlitePool};

/// One-time move from per-product prices to stock batches, run on a database
/// whose `products` still carry `cost_price_cents`/`selling_price_cents`:
/// every product gets an opening batch (`stb_open_<productKey>`, same id and
/// timestamps as the product) holding its current stock at its last cost and
/// price; existing movements, serials and sale lines are pointed at it;
/// purchases get the price their units sold at; then the product price
/// columns are dropped. Every row it writes is derived from the product
/// alone, so a desktop and the cloud migrating separately end up with
/// identical rows that sync merges instead of duplicating. The capture
/// triggers are dropped first (`init_db` reinstalls them right after) so the
/// migration itself is not queued for upload, and the whole move is one
/// transaction. Waits for `stock_batches` to exist, i.e. for the pass after
/// `SCHEMA_SQL`.
pub async fn migrate_to_stock_batches(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let product_columns = table_columns(pool, "products").await?;
    if !product_columns.iter().any(|c| c == "cost_price_cents")
        || table_columns(pool, "stock_batches").await?.is_empty()
    {
        return Ok(());
    }
    tracing::info!("Migrating products to stock batches");

    for (table, column, ddl) in [
        (
            "stock_movements",
            "batch_key",
            "ALTER TABLE stock_movements ADD COLUMN batch_key TEXT",
        ),
        // NOT NULL needs a value for existing rows; every one is rewritten below.
        (
            "product_serials",
            "batch_key",
            "ALTER TABLE product_serials ADD COLUMN batch_key TEXT NOT NULL DEFAULT ''",
        ),
        (
            "purchases",
            "selling_price_cents",
            "ALTER TABLE purchases ADD COLUMN selling_price_cents INTEGER NOT NULL DEFAULT 0",
        ),
    ] {
        if !table_columns(pool, table)
            .await?
            .iter()
            .any(|c| c == column)
        {
            pool.execute(ddl).await?;
        }
    }

    let mut tx = pool.begin().await?;
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(&mut *tx)
    .await?;
    for trigger in triggers {
        tx.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }

    tx.execute(
        "INSERT OR IGNORE INTO stock_batches (
            key, id, product_key, source, purchase_key, supplier_key, received_quantity,
            remaining_quantity, unit_cost_cents, selling_price_cents, received_at, version,
            created_at, updated_at, deleted_at, updated_by_device
         )
         SELECT 'stb_open_' || key, id, key, 'opening', NULL, NULL, stock_quantity,
                stock_quantity, cost_price_cents, selling_price_cents, created_at, 1,
                created_at, created_at, deleted_at, NULL
         FROM products",
    )
    .await?;
    // Audit-only return movements (no quantity) stay without a batch.
    tx.execute(
        "UPDATE stock_movements
         SET batch_key = 'stb_open_' || (SELECT p.key FROM products p WHERE p.id = stock_movements.product_id)
         WHERE batch_key IS NULL AND quantity_delta != 0
           AND EXISTS (SELECT 1 FROM products p WHERE p.id = stock_movements.product_id)",
    )
    .await?;
    tx.execute(
        "UPDATE product_serials SET batch_key = 'stb_open_' || product_key WHERE batch_key = ''",
    )
    .await?;
    tx.execute(
        "UPDATE purchases SET selling_price_cents =
            (SELECT p.selling_price_cents FROM products p WHERE p.key = purchases.product_key)
         WHERE selling_price_cents = 0
           AND EXISTS (SELECT 1 FROM products p WHERE p.key = purchases.product_key)",
    )
    .await?;

    let costs: std::collections::HashMap<String, i64> =
        sqlx::query_as::<_, (String, i64)>("SELECT key, cost_price_cents FROM products")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let cost_of = |key: &str| costs.get(key).copied();
    for (table, columns) in [
        ("invoices", &["items"][..]),
        ("credit_notes", &["returned_items", "exchange_items"][..]),
    ] {
        if table_columns(pool, table).await?.is_empty() {
            continue;
        }
        for column in columns {
            let rows: Vec<(String, Option<String>)> =
                sqlx::query_as(&format!("SELECT key, {column} FROM {table}"))
                    .fetch_all(&mut *tx)
                    .await?;
            for (key, json) in rows {
                let Some(mut items) = json.and_then(|j| serde_json::from_str(&j).ok()) else {
                    continue;
                };
                if crate::modules::inventory::service::stock_batch::backfill_line_allocations(
                    &mut items, cost_of,
                ) {
                    sqlx::query(&format!("UPDATE {table} SET {column} = ? WHERE key = ?"))
                        .bind(items.to_string())
                        .bind(&key)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
    }

    tx.execute("ALTER TABLE products DROP COLUMN cost_price_cents")
        .await?;
    tx.execute("ALTER TABLE products DROP COLUMN selling_price_cents")
        .await?;
    tx.commit().await?;
    Ok(())
}
