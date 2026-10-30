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
    post,
    path = "/{key}/cancel-close",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    responses(
        (status = 200, description = "Closing cancelled, session back in use", body = ApiResponse<RegisterSession>),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 409, description = "Session already closed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn cancel_close(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let session = service::cancel_close(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        session,
        "Closing cancelled successfully",
    )))
}
