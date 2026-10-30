use super::require_caller_role::require_caller_role;
use crate::app::AppState;
use crate::core::constants::{modules, permissions};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::users::{CreateUserRequest, User};
use crate::modules::users::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/", tag = modules::USERS, request_body = CreateUserRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "User created", body = ApiResponse<User>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing permission, or the target role is outside what the caller may create (e.g. a Manager creating a Manager/Admin)", body = ErrorResponse),
        (status = 409, description = "Email already exists, or (creating an Admin) an Admin account already exists", body = ErrorResponse),
    )
)]
pub(super) async fn create_user(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<User>>)> {
    user.require_any_permission(&[permissions::USERS_MANAGE, permissions::USERS_MANAGE_STAFF])?;
    let caller_role = require_caller_role(&user)?;
    let created = service::create_user_for_caller(&state.db, body, caller_role).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(created, "User created successfully")),
    ))
}
