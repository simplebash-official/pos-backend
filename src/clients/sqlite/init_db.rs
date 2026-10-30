use super::migrate_purchases_to_receipts::migrate_purchases_to_receipts;
use super::migrate_schema::migrate_schema;

use sqlx::{Executor, SqlitePool};

const SCHEMA_SQL: &str = include_str!("../sqlite_schema.sql");

/// Runs embedded DDL schema migrations to create all tables and indexes,
/// followed by automatic schema healing/migrations for existing databases.
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Run migrations first so legacy tables have required columns before SCHEMA_SQL creates indexes on them
    migrate_schema(pool).await?;
    pool.execute(SCHEMA_SQL).await?;
    migrate_schema(pool).await?;
    migrate_purchases_to_receipts(pool).await?;
    // Sync v2 device bookkeeping: the single state row and the capture
    // triggers. Both are inert until sync is enabled (`capture_enabled = 1`).
    super::super::sync_capture::ensure_state(pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    super::super::sync_capture::install_triggers(pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    tracing::info!("SQLite database schema and indexes initialized");
    Ok(())
}
