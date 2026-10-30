use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync::{SyncChangesQuery, SyncChangesResponse};
use crate::modules::sync::service;

use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/changes", tag = modules::SYNC, params(SyncChangesQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sync delta changes for syncable resources", body = ApiResponse<SyncChangesResponse>),
        (status = 400, description = "Invalid cursor or query parameter", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_changes(
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
