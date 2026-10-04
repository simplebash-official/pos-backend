use axum::{
    Json,
    extract::{Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        config::TenantMode,
        constants::modules,
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::sync::{SyncChangesQuery, SyncChangesResponse, SyncStatusResponse},
    modules::{sync::service, tenants::repository as tenants_repository},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_changes))
        .routes(routes!(sync_status))
        .merge(crate::modules::sync::routes_v2::router())
        .merge(crate::modules::sync::routes_local::router())
}

#[utoipa::path(get, path = "/changes", tag = modules::SYNC, params(SyncChangesQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sync delta changes for syncable resources", body = ApiResponse<SyncChangesResponse>),
        (status = 400, description = "Invalid cursor or query parameter", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn get_changes(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SyncChangesQuery>,
) -> AppResult<Json<ApiResponse<SyncChangesResponse>>> {
    let is_admin = current_user.role == Some(crate::domain::users::Role::Admin);
    let response = service::get_changes(&state.db, query, is_admin).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Sync changes retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/status", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Per-resource last-modified timestamp and newest cursor", body = ApiResponse<SyncStatusResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn sync_status(
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
