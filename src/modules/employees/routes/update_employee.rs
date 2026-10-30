use super::parse_object_id::parse_object_id;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::employees::{Employee, UpdateEmployeeRequest};
use crate::modules::employees::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(patch, path = "/{id}", tag = modules::EMPLOYEES,
    params(("id" = String, Path, description = "Employee id")),
    request_body = UpdateEmployeeRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Employee updated", body = ApiResponse<Employee>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing employees:write permission", body = ErrorResponse),
        (status = 404, description = "Employee not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
pub(super) async fn update_employee(
    user: CurrentUser,
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdateEmployeeRequest>,
) -> AppResult<Json<ApiResponse<Employee>>> {
    user.require_permission(perm::EMPLOYEES_WRITE)?;
    let object_id = parse_object_id(&id)?;
    let employee =
        service::update_employee(&state.db, object_id, body, if_match.0, device_id.0).await?;

    Ok(Json(ApiResponse::success(
        employee,
        "Employee updated successfully",
    )))
}
