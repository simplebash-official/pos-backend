use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::{CreateCreditNoteRequest, CreditNote};

use crate::modules::billing::service;

use crate::modules::users;
use axum::Json;

use axum::extract::State;

#[utoipa::path(post, path = "/credit-notes", tag = modules::BILLING, request_body = CreateCreditNoteRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Credit note created — stock adjusted, refund/payment and customer balance updated", body = ApiResponse<CreditNote>),
        (status = 400, description = "Validation error or return quantity exceeded", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied, or manager override required (no-receipt / past return window)", body = ErrorResponse),
        (status = 404, description = "Invoice or product not found", body = ErrorResponse),
        (status = 409, description = "Invoice not eligible for a credit note, or past the return window without override", body = ErrorResponse),
    )
)]
pub(super) async fn create_credit_note(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateCreditNoteRequest>,
) -> AppResult<Json<ApiResponse<CreditNote>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let cashier_name = match mongodb::bson::oid::ObjectId::parse_str(&user.user_id) {
        Ok(object_id) => users::service::get_user(&state.db, object_id)
            .await
            .map(|u| u.name)
            .unwrap_or_else(|_| "Unknown".to_string()),
        Err(_) => "Unknown".to_string(),
    };
    let response = service::credit_notes::create_credit_note(
        &state.db,
        &state.config,
        body,
        user.user_id,
        cashier_name,
        user.role,
        device_id.0,
    )
    .await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        response,
        "Credit note processed successfully",
    )))
}
