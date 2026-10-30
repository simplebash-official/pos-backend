use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::users::{UpdateMyPreferencesRequest, User};
use crate::modules::auth::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(patch, path = "/me/preferences", tag = modules::AUTH,
    request_body = UpdateMyPreferencesRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Updated current user (with preferences)", body = ApiResponse<User>),
        (status = 400, description = "Unknown preference value", body = ErrorResponse),
        (status = 401, description = "Missing/invalid/expired token, or account no longer active", body = ErrorResponse),
    )
)]
pub(super) async fn update_my_preferences(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateMyPreferencesRequest>,
) -> AppResult<Json<ApiResponse<User>>> {
    let updated =
        service::update_my_preferences(&state.db, &user.user_id, user.name.as_deref(), body)
            .await?;

    Ok(Json(ApiResponse::success(
        updated,
        "Preferences updated successfully",
    )))
}
