use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{TopCustomersQuery, TopCustomersResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/analytics/top-customers",
    tag = modules::REPORTS,
    params(TopCustomersQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Top customers by revenue, invoices or product margin", body = ApiResponse<TopCustomersResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_analytics_top_customers(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<TopCustomersQuery>,
) -> AppResult<Json<ApiResponse<TopCustomersResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_top_customers(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics top customers retrieved successfully",
    )))
}
