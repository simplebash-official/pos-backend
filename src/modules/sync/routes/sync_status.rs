use crate::app::AppState;
use crate::core::config::TenantMode;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync::SyncStatusResponse;
use crate::modules::sync::service;
use crate::modules::tenants::repository as tenants_repository;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/status", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Per-resource last-modified timestamp and newest cursor", body = ApiResponse<SyncStatusResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn sync_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SyncStatusResponse>>> {
    let mut result = service::get_sync_status(&state.db).await?;
    // A cloud shop reports whether its own first-time setup is done, so a device
    // joining it knows whether to offer the demo/clean choice.
    if state.config.tenant_mode == TenantMode::Multi
        && let Some(tenant_id) = current_user.tenant_id.as_deref()
        && let Some(tenant) = tenants_repository::find_tenant_by_key(&state.db, tenant_id).await?
    {
        result.setup_completed = tenant.setup_completed;
        result.sample_data_loaded = tenant.sample_data_loaded;
    }
    Ok(Json(ApiResponse::success(result, "Sync status retrieved")))
}
