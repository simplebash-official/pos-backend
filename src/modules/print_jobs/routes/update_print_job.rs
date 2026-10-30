use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::print_jobs::{PrintJob, UpdatePrintJobRequest};
use crate::modules::print_jobs::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(patch, path = "/{id}", tag = modules::PRINT_JOBS,
    params(("id" = String, Path, description = "Print job ID or prefixed key")),
    request_body = UpdatePrintJobRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Print job updated", body = ApiResponse<PrintJob>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Print job not found", body = ErrorResponse),
    )
)]
pub(super) async fn update_print_job(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdatePrintJobRequest>,
) -> AppResult<Json<ApiResponse<PrintJob>>> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let print_job = service::update_print_job(&state.db, &id, body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        print_job,
        "Print job updated successfully",
    )))
}
