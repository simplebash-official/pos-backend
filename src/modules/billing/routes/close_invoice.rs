use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::Invoice;

use crate::modules::billing::service::sale;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(post, path = "/invoices/{id}/close", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Invoice closed", body = ApiResponse<Invoice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 409, description = "Invoice is not Paid, or has an open (non-voided) credit note against it", body = ErrorResponse),
    )
)]
pub(super) async fn close_invoice(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Invoice>>> {
    let invoice = sale::close_invoice(&state.db, &id, admin.0.user_id).await?;
    Ok(Json(ApiResponse::success(
        invoice,
        "Invoice closed successfully",
    )))
}
