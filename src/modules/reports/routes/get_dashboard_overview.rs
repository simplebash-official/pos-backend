use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{ReportDateRangeQuery, ReportsDashboardResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/dashboard",
    tag = modules::REPORTS,
    params(ReportDateRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Reports & Profit Intelligence executive dashboard overview", body = ApiResponse<ReportsDashboardResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_dashboard_overview(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ReportDateRangeQuery>,
) -> AppResult<Json<ApiResponse<ReportsDashboardResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_dashboard_overview(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Dashboard overview report retrieved successfully",
    )))
}
