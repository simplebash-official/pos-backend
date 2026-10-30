use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::balances::{CreateAdvanceRecoveryRequest, StaffAdvanceDetail};
use crate::modules::balances::service;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/staff/{employeeKey}/recoveries",
    params(("employeeKey" = String, Path, description = "Employee key")),
    request_body = CreateAdvanceRecoveryRequest,
    responses(
        (status = 201, description = "Recovery recorded", body = ApiResponse<StaffAdvanceDetail>),
        (status = 409, description = "More than the staff member still owes", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
pub(super) async fn recover_advance(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(employee_key): Path<String>,
    Json(body): Json<CreateAdvanceRecoveryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<StaffAdvanceDetail>>)> {
    current_user.require_permission(perm::BALANCES_MANAGE)?;
    let name = current_user.resolve_name(&state.db).await;
    let detail =
        service::recover_advance(&state.db, &employee_key, current_user.user_id, name, body)
            .await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            detail,
            "Recovery recorded successfully",
        )),
    ))
}
