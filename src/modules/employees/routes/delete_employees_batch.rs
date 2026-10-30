use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::employees::{DeleteEmployeesRequest, DeleteEmployeesResponse};
use crate::modules::employees::service;
use axum::Json;
use axum::extract::State;

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
