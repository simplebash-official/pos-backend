use crate::clients::sqlite::generate_id_hex;
use crate::core::error::AppResult;

use chrono::Utc;
use sqlx::{Row, SqlitePool};

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
             (key, id, product_id, batch_key, quantity_delta, movement_type, reference_id, note, version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 'opening_balance', ?, 'Opening balance (ledger migration)', 1, ?, ?)",
        )
        .bind(format!("sm_open_{product_key}"))
        .bind(generate_id_hex())
        .bind(row.get::<String, _>("id"))
        // Stock that predates the ledger is the product's opening batch.
        .bind(crate::core::constants::prefixes::opening_batch_key(&product_key))
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
