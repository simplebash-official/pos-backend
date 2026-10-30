use crate::app::AppState;
use crate::core::config::TenantMode;
use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::{AdminUser, verify_bearer};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::system::{
    SetupStatusResponse, SetupSystemRequest, SetupSystemResponse, SystemInstallation,
};
use crate::domain::users::Role;
use crate::modules::system::service;
use crate::modules::users::service as users_service;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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
    let installation = service::get_installation_info(&state.db, &state.config).await?;
    Ok(Json(ApiResponse::success(
        installation,
        "Installation details retrieved successfully",
    )))
}

// (No module-specific private helpers required)

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
pub(super) async fn get_setup_status(
    State(state): State<AppState>,
    Query(query): Query<SetupStatusQuery>,
    headers: HeaderMap,
) -> AppResult<Json<ApiResponse<SetupStatusResponse>>> {
    if state.config.tenant_mode == TenantMode::Multi {
        // 1. Check if an authenticated user with a tenant is calling
        if let Ok(verified) = verify_bearer(&headers, &state.config).await
            && let Some(tid) = verified.tenant_id()
            && let Some(status) = service::get_tenant_setup_status(&state.db, &tid).await?
        {
            return Ok(Json(ApiResponse::success(
                status,
                "Tenant setup status retrieved successfully",
            )));
        }

        // 2. Check if shop query parameter is supplied
        if let Some(shop_code) = query.shop
            && let Some(status) = service::get_tenant_setup_status(&state.db, &shop_code).await?
        {
            return Ok(Json(ApiResponse::success(
                status,
                "Tenant setup status retrieved successfully",
            )));
        }

        // 3. Fallback for unauthenticated multi-tenant check before shop code is known
        let app_version = crate::core::constants::app_version::APP_VERSION.to_string();
        return Ok(Json(ApiResponse::success(
            SetupStatusResponse {
                setup_completed: false,
                is_first_run: true,
                installation_id: None,
                installed_at: None,
                setup_completed_at: None,
                sample_data_loaded: None,
                app_version,
                platform: "Cloud (Multi-Tenant)".to_string(),
            },
            "Tenant setup status retrieved successfully",
        )));
    }

    let status = service::get_setup_status(&state.db, &state.config).await?;
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
pub(super) async fn perform_setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SetupSystemRequest>,
) -> AppResult<Json<ApiResponse<SetupSystemResponse>>> {
    if state.config.tenant_mode == TenantMode::Multi {
        if let Ok(verified) = verify_bearer(&headers, &state.config).await {
            if verified.role != Some(Role::Admin) {
                return Err(AppError::forbidden(
                    "Only administrators can complete store setup",
                ));
            }
            let tid = verified
                .tenant_id()
                .ok_or_else(|| AppError::unauthorized("Authenticated token has no tenant scope"))?;

            let user = match mongodb::bson::oid::ObjectId::parse_str(&verified.user_id) {
                Ok(oid) => users_service::get_user(&state.db, oid).await.ok(),
                // Identity-server token: the caller is the shop's owner, i.e. its Admin.
                Err(_) => {
                    users_service::find_user_for_identity(&state.db, verified.name.as_deref())
                        .await
                        .ok()
                        .flatten()
                }
            };

            let result =
                service::perform_tenant_setup(&state.db, &tid, body.load_sample_data, user).await?;

            return Ok(Json(ApiResponse::success(
                result,
                "Tenant setup completed successfully",
            )));
        }

        return Err(AppError::unauthorized(
            "Please log in to your store to complete onboarding setup",
        ));
    }

    let result = service::perform_setup(&state.db, &state.config, body).await?;
    Ok(Json(ApiResponse::success(
        result,
        "System setup completed successfully",
    )))
}

/// Routes of the system module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_setup_status))
        .routes(routes!(perform_setup))
        .routes(routes!(get_installation))
}

#[derive(Debug, Deserialize)]
pub struct SetupStatusQuery {
    pub shop: Option<String>,
}
