use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::invoice_balance::{CalcInvoiceBalanceReply, CalcInvoiceBalanceRequest};

use crate::modules::calc::invoice_balance;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/invoice-balance", tag = modules::CALC, request_body = CalcInvoiceBalanceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What has been paid on the invoice and what is still owed", body = ApiResponse<CalcInvoiceBalanceReply>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
/// Any signed-in user, like reading the invoice's payments. Reads the invoice and its payments.
pub(super) async fn calc_invoice_balance(
    _user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcInvoiceBalanceRequest>,
) -> AppResult<Json<ApiResponse<CalcInvoiceBalanceReply>>> {
    use crate::modules::billing::service;

    let invoice = service::get_invoice(&state.db, &body.invoice_key).await?;
    let payments = service::payments::list_payments(&state.db, &invoice.key).await?;
    let reply = invoice_balance::compute(&invoice, &payments);
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
