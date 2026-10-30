use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::{OpenSessionRequest, RegisterSession};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/open",
    request_body = OpenSessionRequest,
    responses(
        (status = 201, description = "Register session opened successfully", body = ApiResponse<RegisterSession>),
        (status = 400, description = "Validation failed", body = ErrorResponse),
        (status = 409, description = "Active session already exists", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn open_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<OpenSessionRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<RegisterSession>>)> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let cashier_name = current_user.resolve_name(&state.db).await;
    let session = service::open_session(
        &state.db,
        current_user.user_id,
        cashier_name,
        body,
        device_id.0,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            session,
            "Register session opened successfully",
        )),
    ))
}
