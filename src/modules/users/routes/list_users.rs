use super::require_caller_role::require_caller_role;
use crate::app::AppState;
use crate::core::constants::{modules, permissions};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::users::{UserListQuery, UsersResponse};
use crate::modules::users::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::USERS, params(UserListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List users — scoped to the roles the caller may manage (Admin: Manager+Staff; Manager: Staff only)", body = ApiResponse<UsersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage / users:manage:staff permission", body = ErrorResponse),
    )
)]
pub(super) async fn list_users(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<UserListQuery>,
) -> AppResult<Json<ApiResponse<UsersResponse>>> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let response = service::list_users(&state.db, query, caller_role).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Users retrieved successfully",
    )))
}
