use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::SetPinRequest;
use crate::domain::users::Role;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/set-pin", tag = modules::AUTH, request_body = SetPinRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "PIN set successfully", body = ApiResponse<bool>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden: only admins can set PIN for other users", body = ErrorResponse),
    )
)]
pub(super) async fn set_pin(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<SetPinRequest>,
) -> AppResult<Json<ApiResponse<bool>>> {
    let target_user_id = match &body.target_user_id {
        Some(id) if !id.trim().is_empty() => {
            if id != &user.user_id && user.role != Some(Role::Admin) {
                return Err(AppError::forbidden(
                    "Only admins can set PIN for other users",
                ));
            }
            id.as_str()
        }
        _ => &user.user_id,
    };

    crate::modules::users::service::set_user_pin(&state.db, target_user_id, &body.pin).await?;
    Ok(Json(ApiResponse::success(true, "PIN set successfully")))
}
