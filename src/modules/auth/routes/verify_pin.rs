use crate::app::AppState;
use crate::core::constants::{codes, modules};
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::{VerifyPinRequest, VerifyPinResponse};
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/verify-pin", tag = modules::AUTH, request_body = VerifyPinRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "PIN verification successful", body = ApiResponse<VerifyPinResponse>),
        (status = 401, description = "Invalid PIN or insufficient permissions", body = ErrorResponse),
    )
)]
pub(super) async fn verify_pin(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<VerifyPinRequest>,
) -> AppResult<Json<ApiResponse<VerifyPinResponse>>> {
    let verified_user = crate::modules::users::service::verify_user_pin(
        &state.db,
        &body.pin,
        body.required_role.as_deref(),
    )
    .await?;

    match verified_user {
        Some(user) => Ok(Json(ApiResponse::success(
            VerifyPinResponse {
                valid: true,
                user_id: Some(user.id),
                user_name: Some(user.name),
                role: Some(user.role.as_str().to_string()),
            },
            "PIN verified successfully",
        ))),
        None => Err(AppError::unauthorized_with_code(
            codes::INVALID_PIN,
            "Invalid PIN or insufficient role permissions",
        )),
    }
}
