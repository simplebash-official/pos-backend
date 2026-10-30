use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::customers::{CustomerListQuery, CustomerListResponse};
use crate::modules::customers::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::CUSTOMERS, params(CustomerListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List customers with search, tag filtration, and pagination", body = ApiResponse<CustomerListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_customers(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<CustomerListQuery>,
) -> AppResult<Json<ApiResponse<CustomerListResponse>>> {
    let response = service::list_customers(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Customers retrieved successfully",
    )))
}
