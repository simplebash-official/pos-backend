use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::utils::parse_object_id as parse_mongo_id;
use crate::domain::employees::{
    CreateEmployeeRequest, DeleteEmployeesRequest, DeleteEmployeesResponse, Employee,
    EmployeeListQuery, EmployeesResponse, UpdateEmployeeRequest,
};
use crate::modules::employees::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(post, path = "/", tag = modules::EMPLOYEES, request_body = CreateEmployeeRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Employee created", body = ApiResponse<Employee>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing employees:write permission", body = ErrorResponse),
    )
)]
pub(super) async fn create_employee(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateEmployeeRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Employee>>)> {
    user.require_permission(perm::EMPLOYEES_WRITE)?;
    let employee = service::create_employee(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            employee,
            "Employee created successfully",
        )),
    ))
}

#[utoipa::path(delete, path = "/{id}", tag = modules::EMPLOYEES,
    params(("id" = String, Path, description = "Employee id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Employee deleted", body = ApiResponse<Employee>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing employees:write permission", body = ErrorResponse),
        (status = 404, description = "Employee not found", body = ErrorResponse),
        (status = 409, description = "Employee still has a login account, or version conflict", body = ErrorResponse),
    )
)]
pub(super) async fn delete_employee(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Employee>>> {
    user.require_permission(perm::EMPLOYEES_WRITE)?;
    let object_id = parse_object_id(&id)?;
    let employee = service::delete_employee(&state.db, object_id, device_id.0).await?;

    Ok(Json(ApiResponse::success(
        employee,
        "Employee deleted successfully",
    )))
}

#[utoipa::path(delete, path = "/batch", tag = modules::EMPLOYEES, request_body = DeleteEmployeesRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Employees deleted", body = ApiResponse<DeleteEmployeesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing employees:write permission", body = ErrorResponse),
    )
)]
pub(super) async fn delete_employees_batch(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<DeleteEmployeesRequest>,
) -> AppResult<Json<ApiResponse<DeleteEmployeesResponse>>> {
    user.require_permission(perm::EMPLOYEES_WRITE)?;
    let deleted_count = service::delete_employees(&state.db, body.ids).await?;

    Ok(Json(ApiResponse::success(
        DeleteEmployeesResponse { deleted_count },
        format!("{deleted_count} employees deleted successfully"),
    )))
}

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

/// Parses a `Path<String>` id param — this is the only place in the file
/// that turns a raw path segment into an `ObjectId`; every handler that
/// needs one calls this before delegating to `service::*`.
pub(super) fn parse_object_id(id: &str) -> AppResult<ObjectId> {
    parse_mongo_id(id, "Employee")
}

/// Routes of the employees module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Employees
        .routes(routes!(list_employees, create_employee))
        .routes(routes!(get_employee, update_employee, delete_employee))
        .routes(routes!(delete_employees_batch))
}

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
