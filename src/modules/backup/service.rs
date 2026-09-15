// Service orchestration and business validation for full data backup and restore.

use crate::{
    app::AppState,
    clients::db::Db,
    core::error::{AppError, AppResult},
    domain::backup::{
        BackupExportData, ExportBackupRequest, RestoreBackupRequest, RestoreBackupResponse,
    },
    modules::backup::repository,
};

/// Supported backup file schema version.
const BACKUP_SCHEMA_VERSION: u32 = 1;

/// Exports all SQLite tables and optional client settings into a unified backup object.
pub(crate) async fn export_backup(
    db: &Db,
    req: ExportBackupRequest,
    app_version: &str,
) -> AppResult<BackupExportData> {
    crate::core::logging::domain::tracked("backup.exported", async move {
        let pool = db.as_sqlite().ok_or_else(|| {
            AppError::validation(
                "Data backup export is currently only supported in SQLite desktop mode",
            )
        })?;

        let tables = repository::export_all_tables_sqlite(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to export database tables: {e}")))?;

        let total_tables = tables.len();
        let total_records: usize = tables.values().map(|rows| rows.len()).sum();

        let settings = if req.include_settings {
            req.settings
        } else {
            None
        };

        Ok(BackupExportData {
            version: BACKUP_SCHEMA_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            app_version: app_version.to_string(),
            environment: "desktop".to_string(),
            total_tables,
            total_records,
            tables,
            settings,
        })
    })
    .await
}

/// Validates and transactionally restores database tables from a backup object.
pub(crate) async fn restore_backup(
    state: &AppState,
    req: RestoreBackupRequest,
) -> AppResult<RestoreBackupResponse> {
    crate::core::logging::domain::tracked("backup.restored", async move {
        let backup = req.backup;

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
