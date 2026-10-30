use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::employees::{CreateEmployeeRequest, Employee};
use crate::modules::employees::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

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
