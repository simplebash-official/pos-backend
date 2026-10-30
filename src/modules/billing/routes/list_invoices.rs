use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::{InvoiceListQuery, InvoiceListResponse};

use crate::modules::billing::service;

use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(get, path = "/invoices", tag = modules::BILLING, params(InvoiceListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List invoices with search, status/customer filtering, and pagination", body = ApiResponse<InvoiceListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_invoices(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<InvoiceListQuery>,
) -> AppResult<Json<ApiResponse<InvoiceListResponse>>> {
    let response = service::list_invoices(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Invoices retrieved successfully",
    )))
}
