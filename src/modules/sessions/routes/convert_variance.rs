use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::{CashMovement, ConvertVarianceRequest};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Path, State};
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/{key}/convert-variance",
    params(
        ("key" = String, Path, description = "Register session key")
    ),
    request_body = ConvertVarianceRequest,
    responses(
        (status = 201, description = "Variance converted to late owner withdrawal", body = ApiResponse<CashMovement>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn convert_variance(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Path(key): Path<String>,
    Json(body): Json<ConvertVarianceRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::REGISTER_SESSIONS_MANAGE)?;
    let movement = service::convert_variance(
        &state.db,
        &key,
        current_user.user_id,
        current_user.name.unwrap_or_default(),
        body,
        device_id.0,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            movement,
            "Variance converted to late owner withdrawal",
        )),
    ))
}
