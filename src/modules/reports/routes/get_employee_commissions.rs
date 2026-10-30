use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{EmployeeCommissionsQuery, EmployeeCommissionsReportResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/employee-commissions",
    tag = modules::REPORTS,
    params(EmployeeCommissionsQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Employee commission payouts and technician job performance report", body = ApiResponse<EmployeeCommissionsReportResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_employee_commissions(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<EmployeeCommissionsQuery>,
) -> AppResult<Json<ApiResponse<EmployeeCommissionsReportResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_employee_commissions_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Employee commissions report retrieved successfully",
    )))
}
