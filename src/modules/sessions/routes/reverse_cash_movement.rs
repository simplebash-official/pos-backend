use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::{CashMovement, ReverseCashMovementRequest};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Path, State};
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/movements/{key}/reverse",
    params(("key" = String, Path, description = "Key of the movement to reverse")),
    request_body = ReverseCashMovementRequest,
    responses(
        (status = 201, description = "The reversing entry", body = ApiResponse<CashMovement>),
        (status = 403, description = "Only a manager can reverse this now", body = ErrorResponse),
        (status = 409, description = "Already reversed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
pub(super) async fn reverse_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Path(key): Path<String>,
    Json(body): Json<ReverseCashMovementRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
    let can_reverse = current_user.has_permission(perm::CASH_MOVEMENTS_REVERSE);
    let name = current_user.resolve_name(&state.db).await;
    let movement = service::reverse_movement(
        &state.db,
        &key,
        current_user.user_id,
        name,
        can_reverse,
        body,
        device_id.0,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            movement,
            "Cash movement reversed successfully",
        )),
    ))
}
