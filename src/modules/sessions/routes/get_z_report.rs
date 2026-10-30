use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sessions::ZReportResponse;

use crate::modules::sessions::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(
    get,
    path = "/{key}/z-report",
    params(
        ("key" = String, Path, description = "Register session key or number")
    ),
    responses(
        (status = 200, description = "Z-Report end of session summary", body = ApiResponse<ZReportResponse>),
        (status = 404, description = "Session not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Register Sessions"
)]
pub(super) async fn get_z_report(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<ZReportResponse>>> {
    current_user.require_permission(perm::REGISTER_SESSIONS_VIEW)?;
    let report = service::generate_z_report(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Z-Report generated successfully",
    )))
}
