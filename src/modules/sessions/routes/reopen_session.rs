use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::{RegisterSession, ReopenSessionRequest};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(
    post,
    path = "/{key}/reopen",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = ReopenSessionRequest,
    responses(
        (status = 200, description = "Register session reopened", body = ApiResponse<RegisterSession>),
        (status = 403, description = "Forbidden: requires manager/admin", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn reopen_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<ReopenSessionRequest>,
) -> AppResult<Json<ApiResponse<RegisterSession>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let cashier_name = current_user.resolve_name(&state.db).await;
    let session = service::reopen_session(
        &state.db,
        &key,
        current_user.user_id,
        cashier_name,
        current_user.role,
        body,
    )
    .await?;

    Ok(Json(ApiResponse::success(
        session,
        "Register session reopened successfully",
    )))
}
