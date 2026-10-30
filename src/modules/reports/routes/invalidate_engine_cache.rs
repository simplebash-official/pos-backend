use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::EngineInvalidateResponse;

use axum::Json;

use axum::extract::State;

#[utoipa::path(
    post,
    path = "/engine/invalidate",
    tag = modules::REPORTS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Engine cache invalidated successfully", body = ApiResponse<EngineInvalidateResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn invalidate_engine_cache(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<EngineInvalidateResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let count = state.reports_engine.invalidate_all();
    Ok(Json(ApiResponse::success(
        EngineInvalidateResponse {
            invalidated_keys_count: count,
            message: format!("Successfully cleared {count} cached analytical entries"),
        },
        "Reports engine cache cleared successfully",
    )))
}
