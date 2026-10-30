use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::customers::Customer;
use crate::modules::customers::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/{id}", tag = modules::CUSTOMERS,
    params(("id" = String, Path, description = "Customer ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get single customer profile", body = ApiResponse<Customer>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Customer not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_customer(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Customer>>> {
    let customer = service::get_customer(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        customer,
        "Customer retrieved successfully",
    )))
}
