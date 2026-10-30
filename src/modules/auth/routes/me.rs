use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::users::User;
use crate::modules::auth::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/me", tag = modules::AUTH,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Current user info", body = ApiResponse<User>),
        (status = 401, description = "Missing/invalid/expired token, or account no longer active", body = ErrorResponse),
    )
)]
pub(super) async fn me(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<User>>> {
    let current = service::me(&state.db, &user.user_id, user.name.as_deref()).await?;

    Ok(Json(ApiResponse::success(
        current,
        "Current user retrieved successfully",
    )))
}
