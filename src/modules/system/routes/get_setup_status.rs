use super::setup_status_query::SetupStatusQuery;
use crate::app::AppState;
use crate::core::config::TenantMode;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::verify_bearer;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::system::SetupStatusResponse;
use crate::modules::system::service;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;

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
        let app_version = std::env::var("APP_VERSION").unwrap_or_else(|_| "0.7.0".to_string());
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

    let status = service::get_setup_status(&state.db).await?;
    Ok(Json(ApiResponse::success(
        status,
        "Setup status retrieved successfully",
    )))
}
