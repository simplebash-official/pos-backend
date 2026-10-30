use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::backup::{BackupExportData, ExportBackupRequest};
use crate::modules::backup::service;
use axum::Json;
use axum::extract::State;

// (No module-specific private helpers required)

/// Export a complete system backup package containing all SQLite tables and settings.
#[utoipa::path(
    post,
    path = "/export",
    tag = modules::BACKUP,
    request_body = ExportBackupRequest,
    responses(
        (status = 200, description = "Backup package generated successfully", body = ApiResponse<BackupExportData>),
        (status = 400, description = "Backup error or non-SQLite environment", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn export_backup(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<ExportBackupRequest>,
) -> AppResult<Json<ApiResponse<BackupExportData>>> {
    // Read app version from environment or default
    let app_version = option_env!("CARGO_PKG_VERSION").unwrap_or("0.2.1");
    let result = service::export_backup(&state.db, body, app_version).await?;
    Ok(Json(ApiResponse::success(
        result,
        "System backup generated successfully",
    )))
}
