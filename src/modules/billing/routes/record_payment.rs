use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::{PaymentRecord, RecordPaymentRequest};

use crate::modules::billing::service;

use crate::modules::users;
use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(post, path = "/invoices/{id}/payments", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    request_body = RecordPaymentRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Payment recorded against the invoice", body = ApiResponse<PaymentRecord>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 409, description = "Invoice already cancelled, or payment exceeds outstanding balance", body = ErrorResponse),
    )
)]
pub(super) async fn record_payment(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<RecordPaymentRequest>,
) -> AppResult<Json<ApiResponse<PaymentRecord>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let invoice = service::get_invoice(&state.db, &id).await?;
    let recorded_by_name = match mongodb::bson::oid::ObjectId::parse_str(&user.user_id) {
        Ok(object_id) => users::service::get_user(&state.db, object_id)
            .await
            .map(|u| u.name)
            .unwrap_or_else(|_| "Unknown".to_string()),
        Err(_) => "Unknown".to_string(),
    };
    let payment = service::payments::record_payment(
        &state.db,
        &invoice.key,
        body,
        user.user_id,
        recorded_by_name,
        device_id.0,
    )
    .await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        payment,
        "Payment recorded successfully",
    )))
}
