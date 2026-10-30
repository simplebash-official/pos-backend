use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::DashboardPulse;

use crate::modules::reports::service;
use axum::Json;

use axum::extract::State;

#[utoipa::path(
    get,
    path = "/dashboard-pulse",
    tag = modules::REPORTS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Dashboard groupings: pipelines, technician load, fast movers, low stock, repairs waiting for pickup and the activity stream", body = ApiResponse<DashboardPulse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
/// Any signed-in user; technicians need `employees:read` and stock needs `inventory:read`, or those parts come back empty.
pub(super) async fn get_dashboard_pulse(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<DashboardPulse>>> {
    let response = service::pulse::get_dashboard_pulse(
        &state.db,
        user.has_permission(perm::EMPLOYEES_READ),
        user.has_permission(perm::INVENTORY_READ),
        user.has_permission(perm::QUOTATIONS_MANAGE),
    )
    .await?;
    Ok(Json(ApiResponse::success(
        response,
        "Dashboard pulse retrieved successfully",
    )))
}
