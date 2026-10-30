use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::labels::{LabelJob, UpdateLabelJobRequest};

use crate::modules::labels::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(patch, path = "/jobs/{key}", tag = modules::LABELS, request_body = UpdateLabelJobRequest,
    params(("key" = String, Path, description = "Print job key (lpj_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The printer step's result was recorded", body = ApiResponse<LabelJob>),
        (status = 400, description = "Outcome must be sent or failed; a reason only with failed", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_JOB_NOT_FOUND", body = ErrorResponse),
        (status = 409, description = "LABEL_JOB_ALREADY_REPORTED: a download, or reported before", body = ErrorResponse),
    )
)]
pub(super) async fn report_job(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateLabelJobRequest>,
) -> AppResult<Json<ApiResponse<LabelJob>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let job = service::jobs::report_job(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        job,
        "Label print updated successfully",
    )))
}
