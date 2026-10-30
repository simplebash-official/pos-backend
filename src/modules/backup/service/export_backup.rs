use super::backup_schema_version::BACKUP_SCHEMA_VERSION;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::backup::{BackupExportData, ExportBackupRequest};
use crate::modules::backup::repository;

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
