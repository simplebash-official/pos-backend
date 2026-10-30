use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::quotations::{QuotationListQuery, QuotationListResponse};

use crate::modules::quotations::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::QUOTATIONS, params(QuotationListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List quotations with search, status filter and pagination", body = ApiResponse<QuotationListResponse>),
        (status = 400, description = "Invalid status filter", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn list_quotations(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<QuotationListQuery>,
) -> AppResult<Json<ApiResponse<QuotationListResponse>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let response = service::list_quotations(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Quotations retrieved successfully",
    )))
}
