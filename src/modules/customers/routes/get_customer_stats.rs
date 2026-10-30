use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::customers::CustomerStats;
use crate::modules::customers::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/stats", tag = modules::CUSTOMERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Customers screen KPI cards: total customers, total balance due, and active-debtor count", body = ApiResponse<CustomerStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_customer_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<CustomerStats>>> {
    let stats = service::get_customer_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Customer stats retrieved successfully",
    )))
}
