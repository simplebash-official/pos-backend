use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::{CreditNote, VoidCreditNoteRequest};

use crate::modules::billing::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(post, path = "/credit-notes/{id}/void", tag = modules::BILLING,
    params(("id" = String, Path, description = "Credit note ID or prefixed key")),
    request_body = VoidCreditNoteRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Credit note voided — payments/customer balance/restock reversed where applicable", body = ApiResponse<CreditNote>),
        (status = 400, description = "A reason is required", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Credit note not found", body = ErrorResponse),
        (status = 409, description = "Already voided", body = ErrorResponse),
    )
)]
pub(super) async fn void_credit_note(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VoidCreditNoteRequest>,
) -> AppResult<Json<ApiResponse<CreditNote>>> {
    let credit_note =
        service::credit_notes::void_credit_note(&state.db, &id, body, admin.0.user_id).await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        credit_note,
        "Credit note voided successfully",
    )))
}
