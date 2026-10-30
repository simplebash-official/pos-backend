use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::employees::{EmployeeListQuery, EmployeesResponse};
use crate::modules::employees::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::EMPLOYEES, params(EmployeeListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List employees", body = ApiResponse<EmployeesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing employees:read permission", body = ErrorResponse),
    )
)]
pub(super) async fn list_employees(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<EmployeeListQuery>,
) -> AppResult<Json<ApiResponse<EmployeesResponse>>> {
    user.require_permission(perm::EMPLOYEES_READ)?;
    let response = service::list_employees(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Employees retrieved successfully",
    )))
}
