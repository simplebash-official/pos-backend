use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{SalesByCategoryQuery, SalesByCategoryResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/analytics/sales-by-category",
    tag = modules::REPORTS,
    params(SalesByCategoryQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Retail sales rolled up by product category or subcategory", body = ApiResponse<SalesByCategoryResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_analytics_sales_by_category(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SalesByCategoryQuery>,
) -> AppResult<Json<ApiResponse<SalesByCategoryResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_sales_by_category(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics sales by category retrieved successfully",
    )))
}
