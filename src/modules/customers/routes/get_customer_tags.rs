use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::customers::CustomerTagsResponse;
use crate::modules::customers::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/tags", tag = modules::CUSTOMERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Distinct customer tags in use", body = ApiResponse<CustomerTagsResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_customer_tags(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<CustomerTagsResponse>>> {
    let response = service::get_customer_tags(&state.db).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Customer tags retrieved successfully",
    )))
}
