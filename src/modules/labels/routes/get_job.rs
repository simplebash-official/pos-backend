use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::labels::LabelJob;

use crate::modules::labels::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(get, path = "/jobs/{key}", tag = modules::LABELS,
    params(("key" = String, Path, description = "Print job key (lpj_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "One recorded print", body = ApiResponse<LabelJob>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_JOB_NOT_FOUND", body = ErrorResponse),
    )
)]
pub(super) async fn get_job(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<LabelJob>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let job = service::jobs::get_job(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        job,
        "Label print retrieved successfully",
    )))
}
