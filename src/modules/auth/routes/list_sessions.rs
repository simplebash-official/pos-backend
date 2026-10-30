use crate::app::AppState;
use crate::core::constants::{modules, permissions};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::auth::{LoginSessionListQuery, LoginSessionsResponse};
use crate::modules::auth::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/sessions", tag = modules::AUTH, params(LoginSessionListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Login session history", body = ApiResponse<LoginSessionsResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing sessions:view permission", body = ErrorResponse),
    )
)]
pub(super) async fn list_sessions(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LoginSessionListQuery>,
) -> AppResult<Json<ApiResponse<LoginSessionsResponse>>> {
    user.require_permission(permissions::SESSIONS_VIEW)?;
    let response = service::list_sessions(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Login sessions retrieved successfully",
    )))
}
