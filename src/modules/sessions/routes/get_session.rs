use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::RegisterSession;

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(
    get,
    path = "/{key}",
    params(
        ("key" = String, Path, description = "Unique register session key or number")
    ),
    responses(
        (status = 200, description = "Register session details", body = ApiResponse<RegisterSession>),
        (status = 404, description = "Session not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let session = service::get_session(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        session,
        "Register session details retrieved successfully",
    )))
}
