use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::system::SystemInstallation;
use crate::modules::system::service;
use axum::Json;
use axum::extract::State;

/// Inspect system installation metadata (requires Admin).
#[utoipa::path(
    get,
    path = "/installation",
    tag = modules::SYSTEM,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Installation details retrieved successfully", body = ApiResponse<SystemInstallation>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub(super) async fn get_installation(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> AppResult<Json<ApiResponse<SystemInstallation>>> {
    let installation = service::get_installation_info(&state.db).await?;
    Ok(Json(ApiResponse::success(
        installation,
        "Installation details retrieved successfully",
    )))
}
