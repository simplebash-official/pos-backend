use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{EngineFeedQuery, EngineFeedResponse};

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/engine/feed",
    tag = modules::REPORTS,
    params(EngineFeedQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Unified high-speed analytics & chart engine feed", body = ApiResponse<EngineFeedResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_engine_feed(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<EngineFeedQuery>,
) -> AppResult<Json<ApiResponse<EngineFeedResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = state.reports_engine.get_feed(query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Engine feed retrieved successfully",
    )))
}
