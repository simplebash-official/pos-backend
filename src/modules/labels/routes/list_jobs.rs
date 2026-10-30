use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::labels::{LabelJobListQuery, LabelJobListResponse};

use crate::modules::labels::service;

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(get, path = "/jobs", tag = modules::LABELS, params(LabelJobListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Recorded prints and downloads, newest first", body = ApiResponse<LabelJobListResponse>),
        (status = 400, description = "A date is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn list_jobs(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<LabelJobListQuery>,
) -> AppResult<Json<ApiResponse<LabelJobListResponse>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let jobs = service::jobs::list_jobs(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        jobs,
        "Label prints retrieved successfully",
    )))
}
