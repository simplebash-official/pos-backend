// HTTP route handlers and OpenAPI declarations for system setup and installation status.

use axum::{Json, extract::State};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::AdminUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::system::{
        SetupStatusResponse, SetupSystemRequest, SetupSystemResponse, SystemInstallation,
    },
    modules::system::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_setup_status))
        .routes(routes!(perform_setup))
        .routes(routes!(get_installation))
}

// ============================================================================
// Helpers
// ============================================================================

// (No module-specific private helpers required)

// ============================================================================
// System Setup & Installation Handlers
// ============================================================================

/// Query initial setup and installation status of the POS system.
#[utoipa::path(
    get,
    path = "/setup-status",
    tag = modules::SYSTEM,
    responses(
        (status = 200, description = "Setup status retrieved successfully", body = ApiResponse<SetupStatusResponse>),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
async fn get_setup_status(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SetupStatusResponse>>> {
    let status = service::get_setup_status(&state.db).await?;
    Ok(Json(ApiResponse::success(
        status,
        "Setup status retrieved successfully",
    )))
}

/// Execute initial system setup, bootstrap the administrator account, and conditionally seed database.
#[utoipa::path(
    post,
    path = "/setup",
    tag = modules::SYSTEM,
    request_body = SetupSystemRequest,
    responses(
        (status = 200, description = "System setup completed successfully", body = ApiResponse<SetupSystemResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 409, description = "Setup already completed", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
async fn perform_setup(
    State(state): State<AppState>,
    Json(body): Json<SetupSystemRequest>,
) -> AppResult<Json<ApiResponse<SetupSystemResponse>>> {
    let result = service::perform_setup(&state.db, &state.config, body).await?;
    Ok(Json(ApiResponse::success(
        result,
        "System setup completed successfully",
    )))
}

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
async fn get_installation(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> AppResult<Json<ApiResponse<SystemInstallation>>> {
    let installation = service::get_installation_info(&state.db).await?;
    Ok(Json(ApiResponse::success(
        installation,
        "Installation details retrieved successfully",
    )))
}
