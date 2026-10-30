use super::parse_object_id::parse_object_id;
use super::require_caller_role::require_caller_role;
use crate::app::AppState;
use crate::core::constants::{modules, permissions};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::users::User;
use crate::modules::users::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(delete, path = "/{id}", tag = modules::USERS,
    params(("id" = String, Path, description = "User id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "User deleted", body = ApiResponse<User>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing users:manage / users:manage:staff permission", body = ErrorResponse),
        (status = 404, description = "User not found, or outside what the caller may manage", body = ErrorResponse),
    )
)]
pub(super) async fn delete_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<User>>> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let object_id = parse_object_id(&id)?;
    let deleted = service::delete_user(&state.db, object_id, caller_role).await?;

    Ok(Json(ApiResponse::success(
        deleted,
        "User deleted successfully",
    )))
}
