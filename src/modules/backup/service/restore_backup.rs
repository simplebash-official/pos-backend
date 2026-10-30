use super::backup_schema_version::BACKUP_SCHEMA_VERSION;
use super::upgrade_pre_batch_backup::upgrade_pre_batch_backup;
use super::upgrade_pre_line_total_backup::upgrade_pre_line_total_backup;
use super::upgrade_pre_receipt_backup::upgrade_pre_receipt_backup;
use crate::app::AppState;
use crate::core::error::{AppError, AppResult};
use crate::domain::backup::{RestoreBackupRequest, RestoreBackupResponse};
use crate::modules::backup::repository;

/// Validates and transactionally restores database tables from a backup object.
pub(crate) async fn restore_backup(
    state: &AppState,
    req: RestoreBackupRequest,
) -> AppResult<RestoreBackupResponse> {
    crate::core::logging::domain::tracked("backup.restored", async move {
        let mut backup = req.backup;
        upgrade_pre_batch_backup(&mut backup.tables);
        upgrade_pre_receipt_backup(&mut backup.tables);
        upgrade_pre_line_total_backup(&mut backup.tables);

        if backup.version != BACKUP_SCHEMA_VERSION {
            return Err(AppError::validation(format!(
                "Unsupported backup schema version: {} (expected {})",
                backup.version, BACKUP_SCHEMA_VERSION
            )));
        }

        let pool = state.db.as_sqlite().ok_or_else(|| {
            AppError::validation("Data restore is currently only supported in SQLite desktop mode")
        })?;

        let restored_counts = repository::restore_all_tables_sqlite(pool, &backup.tables)
            .await
            .map_err(|e| {
                AppError::validation(format!("Failed to restore database from backup: {e}"))
            })?;

        let total_tables = restored_counts.len();
        let total_records: usize = restored_counts.values().sum();

        // Invalidate active report caches so live dashboards immediately reflect the restored state.
        state.reports_engine.invalidate_active();

        Ok(RestoreBackupResponse {
            restored_at: chrono::Utc::now().to_rfc3339(),
            total_tables,
            total_records,
            restored_counts,
            settings: backup.settings,
        })
    })
    .await
}
