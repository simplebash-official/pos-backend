use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::{CashMovement, CreateCashMovementRequest};

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/movements",
    request_body = CreateCashMovementRequest,
    responses(
        (status = 201, description = "Cash movement recorded", body = ApiResponse<CashMovement>),
        (status = 400, description = "Validation failed", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Movements"
)]
pub(super) async fn create_cash_movement(
    State(state): State<AppState>,
    current_user: CurrentUser,
    device_id: DeviceId,
    Json(body): Json<CreateCashMovementRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CashMovement>>)> {
    current_user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
    if service::requires_managed_permission(body.movement_type) {
        current_user.require_permission(perm::CASH_MOVEMENTS_CREATE_MANAGED)?;
    }
    let cashier_name = current_user.resolve_name(&state.db).await;
    let movement = service::record_movement(
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
            movement,
            "Cash movement recorded successfully",
        )),
    ))
}
