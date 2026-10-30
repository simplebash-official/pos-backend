use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::BillingStats;

use crate::modules::billing::service;

use axum::Json;

use axum::extract::State;

#[utoipa::path(get, path = "/invoices/stats", tag = modules::BILLING,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sales & Invoices History KPI cards: today's sales, today's invoice count, outstanding credit, and average basket value", body = ApiResponse<BillingStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_invoice_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<BillingStats>>> {
    let stats = service::get_billing_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Billing stats retrieved successfully",
    )))
}
