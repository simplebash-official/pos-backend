use super::apply_records_in_tx::apply_records_in_tx;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::sync_v2::{
    ApplyMode, ApplyOrigin, ApplyRequest, ApplyResponse, ChangeRecord, DerivedScope,
};

use crate::modules::sync::resources::{Phase, SYNC_RESOURCES};

use crate::modules::sync::derived_sqlite;
use crate::modules::sync::state::pool_of;

use sqlx::SqliteConnection;

/// Empties every synced table (first page of a bootstrap).
async fn wipe_synced_tables(conn: &mut SqliteConnection) -> AppResult<()> {
    for spec in SYNC_RESOURCES
        .iter()
        .filter(|s| s.phase == Phase::P3a)
        .rev()
    {
        sqlx::query(&format!("DELETE FROM {}", spec.table))
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("DELETE FROM subcategories")
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM sync_outbox")
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM sync_row_meta")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A device that joined a cloud shop which is already set up has no local "first
/// run" left to do: the shop's admin arrived with the snapshot. Marks the
/// installation as set up so the POS shows its sign-in instead of the wizard.
/// Runs on the final bootstrap page, in the same transaction as the cursor, so a
/// half-downloaded shop is never marked as ready.
///
/// Only when the cloud says the shop's own setup is done (`cloud_setup_completed`)
/// *and* an active admin arrived. A freshly provisioned shop has an admin but has
/// not chosen demo vs clean data yet: it is left un-set-up here, so the wizard
/// asks for that choice (with the cloud admin's password, see `perform_setup`).
async fn adopt_cloud_setup(
    tx: &mut sqlx::SqliteConnection,
    cloud_setup_completed: Option<bool>,
    cloud_sample_data_loaded: Option<bool>,
) -> AppResult<()> {
    if cloud_setup_completed != Some(true) {
        return Ok(());
    }
    let admins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users \
         WHERE role = 'admin' AND is_active = 1 AND deleted_at IS NULL",
    )
    .fetch_one(&mut *tx)
    .await?;
    if admins == 0 {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    // The snapshot replaced whatever demo rows were here; the flag follows the cloud's.
    let sample = i64::from(cloud_sample_data_loaded.unwrap_or(false));
    let updated = sqlx::query(
        "UPDATE system_installations \
         SET setup_completed = 1, sample_data_loaded = ?2, \
             setup_completed_at = COALESCE(setup_completed_at, ?1), updated_at = ?1",
    )
    .bind(&now)
    .bind(sample)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 0 {
        // No installation row yet (setup was never opened): create it, already complete.
        let installation_id = std::env::var("INSTALLATION_ID")
            .unwrap_or_else(|_| format!("inst_{}", nanoid::nanoid!(16)));
        let app_version = std::env::var("APP_VERSION").unwrap_or_else(|_| "0.5.0".to_string());
        let platform =
            std::env::var("PLATFORM").unwrap_or_else(|_| std::env::consts::OS.to_string());
        sqlx::query(
            "INSERT INTO system_installations \
             (key, id, installation_id, app_version, platform, installed_at, setup_completed, \
              setup_completed_at, sample_data_loaded, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?6, ?7, ?6, ?6)",
        )
        .bind(format!("inst_{}", nanoid::nanoid!(16)))
        .bind(format!("inst_{}", nanoid::nanoid!(16)))
        .bind(installation_id)
        .bind(app_version)
        .bind(platform)
        .bind(&now)
        .bind(sample)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

/// `POST /api/sync/apply`: one page of pulled changes, in one transaction.
pub async fn apply_batch(db: &Db, req: ApplyRequest) -> AppResult<ApplyResponse> {
    crate::core::logging::domain::tracked("sync.applied", async move {
        let pool = pool_of(db)?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("UPDATE sync_state SET applying = 1 WHERE id = 1")
            .execute(&mut *tx)
            .await?;

        if req.mode == ApplyMode::Bootstrap {
            let active: i64 =
                sqlx::query_scalar("SELECT bootstrap_active FROM sync_state WHERE id = 1")
                    .fetch_one(&mut *tx)
                    .await?;
            if active == 0 {
                wipe_synced_tables(&mut tx).await?;
                sqlx::query("UPDATE sync_state SET bootstrap_active = 1 WHERE id = 1")
                    .execute(&mut *tx)
                    .await?;
            }
        }

        let records: Vec<&ChangeRecord> = req.changes.iter().map(|c| &c.record).collect();
        let mut scope = DerivedScope::default();
        let mut conflicts = Vec::new();
        let counts = apply_records_in_tx(
            db,
            &mut tx,
            &records,
            &ApplyOrigin::Cloud,
            &mut scope,
            &mut conflicts,
        )
        .await?;

        conflicts.extend(derived_sqlite::recompute(&mut tx, &scope).await?);
        derived_sqlite::store_conflicts(&mut tx, &conflicts).await?;

        if let Some(cursor) = req.advance_cursor_to {
            sqlx::query(
                "UPDATE sync_state SET cloud_cursor = ?, bootstrap_active = 0 WHERE id = 1",
            )
            .bind(cursor)
            .execute(&mut *tx)
            .await?;
            if req.mode == ApplyMode::Bootstrap {
                adopt_cloud_setup(&mut tx, req.setup_completed, req.sample_data_loaded).await?;
            }
        }
        // Cleared inside the same transaction: a rollback reverts it too.
        sqlx::query("UPDATE sync_state SET applying = 0 WHERE id = 1")
            .execute(&mut *tx)
            .await?;
        let cursor: i64 = sqlx::query_scalar("SELECT cloud_cursor FROM sync_state WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(ApplyResponse {
            applied: counts.applied,
            duplicates: counts.duplicates,
            conflicts: counts.conflicts + conflicts.len() as u32,
            cursor,
        })
    })
    .await
}
