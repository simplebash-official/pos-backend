use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::balances::StaffAdvanceDetail;
use crate::modules::balances::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    get,
    path = "/staff/{employeeKey}",
    params(("employeeKey" = String, Path, description = "Employee key")),
    responses(
        (status = 200, description = "One staff member's advances given and recovered", body = ApiResponse<StaffAdvanceDetail>),
        (status = 404, description = "Employee not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Balances"
)]
pub(super) async fn get_staff_advance(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(employee_key): Path<String>,
) -> AppResult<Json<ApiResponse<StaffAdvanceDetail>>> {
    current_user.require_permission(perm::BALANCES_VIEW)?;
    let detail = service::get_staff_advance(&state.db, &employee_key).await?;
    Ok(Json(ApiResponse::success(
        detail,
        "Staff advance retrieved successfully",
    )))
}
