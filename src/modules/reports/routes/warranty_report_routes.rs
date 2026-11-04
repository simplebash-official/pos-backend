//! `GET /reports/warranties`: the warranty report, behind its own permission.

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::reports::AnalyticsRangeQuery;
use crate::domain::warranties::WarrantyReport;
use crate::modules::reports::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/warranties",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Claim rates by product, repair service and technician for warranties given in the period, supplier trips started in it, and rework cost by month", body = ApiResponse<WarrantyReport>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// The warranty report for a period.
pub(crate) async fn get_warranty_report(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<WarrantyReport>>> {
    user.require_permission(perm::WARRANTY_REPORTS_VIEW)?;
    let report = service::get_warranty_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Warranty report retrieved",
    )))
}
