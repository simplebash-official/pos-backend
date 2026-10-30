use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::backup::{RestoreBackupRequest, RestoreBackupResponse};
use crate::modules::backup::service;
use axum::Json;
use axum::extract::State;

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
