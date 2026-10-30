use super::parse_object_id::parse_object_id;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::employees::Employee;
use crate::modules::employees::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/{id}", tag = modules::EMPLOYEES,
    params(("id" = String, Path, description = "Employee id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get an employee", body = ApiResponse<Employee>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing employees:read permission", body = ErrorResponse),
        (status = 404, description = "Employee not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_employee(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Employee>>> {
    user.require_permission(perm::EMPLOYEES_READ)?;
    let object_id = parse_object_id(&id)?;
    let employee = service::get_employee(&state.db, object_id).await?;

    Ok(Json(ApiResponse::success(
        employee,
        "Employee retrieved successfully",
    )))
}
