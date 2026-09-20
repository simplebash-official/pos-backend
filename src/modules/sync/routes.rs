use axum::{
    Json,
    extract::{Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::sync::{SyncChangesQuery, SyncChangesResponse, SyncStatusResponse},
    modules::sync::service,
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
    _current_user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SyncChangesQuery>,
) -> AppResult<Json<ApiResponse<SyncChangesResponse>>> {
    let response = service::get_changes(&state.db, query).await?;
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
    _current_user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SyncStatusResponse>>> {
    let result = service::get_sync_status(&state.db).await?;
    Ok(Json(ApiResponse::success(result, "Sync status retrieved")))
}
