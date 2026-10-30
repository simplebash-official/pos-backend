use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::backup::{
    BackupExportData, ExportBackupRequest, RestoreBackupRequest, RestoreBackupResponse,
};
use crate::modules::backup::service;
use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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
    let app_version = crate::core::constants::app_version::APP_VERSION;
    let result = service::export_backup(&state.db, body, app_version).await?;
    Ok(Json(ApiResponse::success(
        result,
        "System backup generated successfully",
    )))
}

/// Transactionally restore all system tables from a previously exported backup.
#[utoipa::path(
    post,
    path = "/import",
    tag = modules::BACKUP,
    request_body = RestoreBackupRequest,
    responses(
        (status = 200, description = "System restored successfully from backup", body = ApiResponse<RestoreBackupResponse>),
        (status = 400, description = "Validation error or foreign key violation", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn restore_backup(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<RestoreBackupRequest>,
) -> AppResult<Json<ApiResponse<RestoreBackupResponse>>> {
    let result = service::restore_backup(&state, body).await?;
    Ok(Json(ApiResponse::success(
        result,
        "System restored successfully from backup",
    )))
}

/// Routes of the backup module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(export_backup))
        .routes(routes!(restore_backup))
        .layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024))
}
