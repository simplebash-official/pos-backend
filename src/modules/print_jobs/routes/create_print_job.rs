use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::print_jobs::{CreatePrintJobRequest, PrintJob};
use crate::modules::print_jobs::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/", tag = modules::PRINT_JOBS, request_body = CreatePrintJobRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Print job created", body = ApiResponse<PrintJob>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn create_print_job(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreatePrintJobRequest>,
) -> AppResult<Json<ApiResponse<PrintJob>>> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let print_job = service::create_print_job(&state.db, body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        print_job,
        "Print job created successfully",
    )))
}
