use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{TopProductsQuery, TopProductsResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/top-products",
    tag = modules::REPORTS,
    params(TopProductsQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Top selling products report by volume or revenue", body = ApiResponse<TopProductsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_top_products(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<TopProductsQuery>,
) -> AppResult<Json<ApiResponse<TopProductsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_top_products(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Top products report retrieved successfully",
    )))
}
