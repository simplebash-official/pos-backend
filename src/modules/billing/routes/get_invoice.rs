use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::Invoice;

use crate::modules::billing::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(get, path = "/invoices/{id}", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get an invoice", body = ApiResponse<Invoice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_invoice(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Invoice>>> {
    let invoice = service::get_invoice(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        invoice,
        "Invoice retrieved successfully",
    )))
}
