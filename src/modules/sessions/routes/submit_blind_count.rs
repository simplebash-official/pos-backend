use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::{RegisterSession, SubmitBlindCountRequest};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(
    post,
    path = "/{key}/blind-count",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = SubmitBlindCountRequest,
    responses(
        (status = 200, description = "Blind count recorded", body = ApiResponse<RegisterSession>),
        (status = 404, description = "Session not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn submit_blind_count(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<SubmitBlindCountRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let session = service::submit_blind_count(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        session,
        "Blind count recorded successfully",
    )))
}
