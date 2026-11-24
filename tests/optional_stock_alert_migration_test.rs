// A SQLite database made when every product had to carry a low-stock alert level
// (`min_stock_threshold INTEGER NOT NULL DEFAULT 0`) becomes one where the level is optional,
// with every stored level kept exactly as it is.

use simplebash_pos_backend::clients::sqlite::{init_db, migrate_optional_stock_alert};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

async fn pool() -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("an in-memory database opens")
}

/// Whether `products.min_stock_threshold` refuses null.
async fn level_is_required(pool: &SqlitePool) -> bool {
    let required: i64 = sqlx::query_scalar(
        "SELECT \"notnull\" FROM pragma_table_info('products') WHERE name = 'min_stock_threshold'",
    )
    .fetch_one(pool)
    .await
    .expect("the column exists");
    required == 1
}

/// The old shape: the level required, a sync trigger that names it.
async fn old_products_table(pool: &SqlitePool) {
    sqlx::query(
        "CREATE TABLE products (
            key TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            stock_quantity INTEGER NOT NULL DEFAULT 0,
            min_stock_threshold INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await
    .expect("the old table is created");
    sqlx::query("CREATE TABLE changes (note TEXT)")
        .execute(pool)
        .await
        .expect("the log table is created");
    sqlx::query(
        "CREATE TRIGGER sync_products_after_update AFTER UPDATE ON products
         BEGIN INSERT INTO changes VALUES (NEW.key || NEW.min_stock_threshold); END",
    )
    .execute(pool)
    .await
    .expect("the trigger is created");
    for (key, level) in [("prod_a", 5), ("prod_b", 0), ("prod_c", 12)] {
        sqlx::query("INSERT INTO products (key, name, min_stock_threshold) VALUES (?, ?, ?)")
            .bind(key)
            .bind(key)
            .bind(level)
            .execute(pool)
            .await
            .expect("a product is stored");
    }
}

#[tokio::test]
async fn the_level_becomes_optional_and_every_stored_level_is_kept() {
    let pool = pool().await;
    old_products_table(&pool).await;
    assert!(level_is_required(&pool).await);

    migrate_optional_stock_alert(&pool)
        .await
        .expect("the migration runs");

    assert!(!level_is_required(&pool).await, "the level may now be null");
    let levels: Vec<(String, Option<i64>)> =
        sqlx::query_as("SELECT key, min_stock_threshold FROM products ORDER BY key")
            .fetch_all(&pool)
            .await
            .expect("the products are read");
    assert_eq!(
        levels,
        [
            ("prod_a".to_string(), Some(5)),
            ("prod_b".to_string(), Some(0)),
            ("prod_c".to_string(), Some(12)),
        ],
        "an existing 0 stays 0: it is a real level, not a missing one"
    );
    let triggers: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_one(&pool)
    .await
    .expect("the triggers are counted");
    assert_eq!(triggers, 0, "init_db installs them again right after");

    sqlx::query(
        "INSERT INTO products (key, name, min_stock_threshold) VALUES ('prod_d', 'd', NULL)",
    )
    .execute(&pool)
    .await
    .expect("a product with no level can be stored");
    let none: Option<i64> =
        sqlx::query_scalar("SELECT min_stock_threshold FROM products WHERE key = 'prod_d'")
            .fetch_one(&pool)
            .await
            .expect("the product is read");
    assert_eq!(none, None);
}

#[tokio::test]
async fn running_it_again_changes_nothing() {
    let pool = pool().await;
    old_products_table(&pool).await;
    migrate_optional_stock_alert(&pool)
        .await
        .expect("the first run works");
    sqlx::query("UPDATE products SET min_stock_threshold = NULL WHERE key = 'prod_a'")
        .execute(&pool)
        .await
        .expect("a level is removed");

    migrate_optional_stock_alert(&pool)
        .await
        .expect("the second run works");

    let level: Option<i64> =
        sqlx::query_scalar("SELECT min_stock_threshold FROM products WHERE key = 'prod_a'")
            .fetch_one(&pool)
            .await
            .expect("the product is read");
    assert_eq!(level, None, "a removed level is not brought back");
}

#[tokio::test]
async fn a_new_database_starts_with_an_optional_level() {
    let pool = pool().await;
    init_db(&pool).await.expect("the schema is created");
    assert!(!level_is_required(&pool).await);
    migrate_optional_stock_alert(&pool)
        .await
        .expect("a database that is already optional is left alone");
}
