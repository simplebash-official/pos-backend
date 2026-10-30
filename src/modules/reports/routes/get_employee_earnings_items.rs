use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::employees::EmployeeEarningsResponse;
use crate::domain::reports::EmployeeCommissionsQuery;

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/employee-commissions/{employeeKey}/items",
    tag = modules::REPORTS,
    params(("employeeKey" = String, Path, description = "Employee key"), EmployeeCommissionsQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Itemized commission line items for one employee", body = ApiResponse<EmployeeEarningsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Employee not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_employee_earnings_items(
    user: CurrentUser,
    State(state): State<AppState>,
    axum::extract::Path(employee_key): axum::extract::Path<String>,
    Query(query): Query<EmployeeCommissionsQuery>,
) -> AppResult<Json<ApiResponse<EmployeeEarningsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_employee_earnings(&state.db, &employee_key, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Employee earnings retrieved successfully",
    )))
}
