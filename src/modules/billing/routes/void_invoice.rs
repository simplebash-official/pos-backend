use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::{Invoice, VoidInvoiceRequest};

use crate::modules::billing::service::sale;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(post, path = "/invoices/{id}/void", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    request_body = VoidInvoiceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Invoice voided — stock and customer balance reversed where applicable", body = ApiResponse<Invoice>),
        (status = 400, description = "A reason is required", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 409, description = "Already voided, or has payments recorded beyond the original sale", body = ErrorResponse),
    )
)]
pub(super) async fn void_invoice(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VoidInvoiceRequest>,
) -> AppResult<Json<ApiResponse<Invoice>>> {
    let (invoice, warnings) = sale::void_invoice(&state.db, &id, body, admin.0.user_id).await?;
    state.reports_engine.invalidate_active();
    let message = if warnings.is_empty() {
        "Invoice voided successfully".to_string()
    } else {
        format!("Invoice voided with warnings: {}", warnings.join("; "))
    };
    Ok(Json(ApiResponse::success(invoice, message)))
}
