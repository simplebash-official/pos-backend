use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::calc::credit_note::{
    CalcCreditNoteReply, CalcCreditNoteRequest, InvoiceContext,
};

use crate::modules::calc::credit_note;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/credit-note", tag = modules::CALC, request_body = CalcCreditNoteRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What each returned unit is worth back, the net, the cash refund, where it goes and whether the return window has passed", body = ApiResponse<CalcCreditNoteReply>),
        (status = 400, description = "A line is not on the invoice, too many units are returned, or an amount is out of range", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
/// Same access as creating a credit note (`billing:write`). Reads the invoice and its payments;
/// writes nothing.
pub(super) async fn calc_credit_note(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CalcCreditNoteRequest>,
) -> AppResult<Json<ApiResponse<CalcCreditNoteReply>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let window = state.config.return_window_days;
    let reply = match &body.invoice_key {
        Some(key) => {
            let invoice = crate::modules::billing::service::get_invoice(&state.db, key).await?;
            let payments = crate::modules::billing::service::credit_notes::invoice_payment_legs(
                &state.db,
                &invoice.key,
            )
            .await?;
            let days = (chrono::Utc::now() - invoice.created_at).num_days();
            let context = InvoiceContext {
                invoice: &invoice,
                payments: &payments,
            };
            credit_note::compute(&body, Some(&context), Some(days), window)?
        }
        None => credit_note::compute(&body, None, None, window)?,
    };
    Ok(Json(ApiResponse::success(reply, "Calculated")))
}
