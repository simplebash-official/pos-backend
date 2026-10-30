use super::apply_records_in_tx::apply_records_in_tx;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sync_v2::{ApplyOrigin, ApplyOutcome, ChangeRecord, DerivedScope};

use crate::modules::sync::derived_sqlite;

use sqlx::SqlitePool;

/// SQLite arm of the per-module `apply_sync_documents` functions: applies the
/// given records in their own transaction and reports what happened.
pub async fn apply_records(
    db: &Db,
    pool: &SqlitePool,
    changes: &[&ChangeRecord],
    origin: ApplyOrigin,
) -> AppResult<ApplyOutcome> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("UPDATE sync_state SET applying = 1 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    let mut scope = DerivedScope::default();
    let mut conflicts = Vec::new();
    let counts =
        apply_records_in_tx(db, &mut tx, changes, &origin, &mut scope, &mut conflicts).await?;
    conflicts.extend(derived_sqlite::recompute(&mut tx, &scope).await?);
    derived_sqlite::store_conflicts(&mut tx, &conflicts).await?;
    sqlx::query("UPDATE sync_state SET applying = 0 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ApplyOutcome {
        applied: counts.applied,
        duplicates: counts.duplicates,
        conflicts,
        touched: scope,
    })
}
