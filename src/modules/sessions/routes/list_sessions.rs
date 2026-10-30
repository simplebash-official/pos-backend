use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::ApiResponse;
use crate::domain::sessions::{SessionListQuery, SessionListResponse};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "",
    params(SessionListQuery),
    responses(
        (status = 200, description = "List of register sessions", body = ApiResponse<SessionListResponse>)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn list_sessions(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<SessionListQuery>,
) -> AppResult<Json<ApiResponse<SessionListResponse>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let response = service::list_sessions(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Register sessions retrieved successfully",
    )))
}
