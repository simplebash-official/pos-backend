use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::purchase_receipts::{CreateReceiptRequest, PaymentSource, PurchaseReceipt};

use crate::modules::purchases::service::receipt;
use crate::modules::purchases::service::receipt::RecordedBy;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/", tag = modules::PURCHASE_RECEIPTS, request_body = CreateReceiptRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Stock received and recorded", body = ApiResponse<PurchaseReceipt>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or product not found", body = ErrorResponse),
        (status = 409, description = "Duplicate invoice number or serial number", body = ErrorResponse),
    )
)]
pub(super) async fn record_receipt(
    AdminUser(current_user): AdminUser,
    State(state): State<AppState>,
    device_id: DeviceId,
    Json(body): Json<CreateReceiptRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<PurchaseReceipt>>)> {
    // What is paid as the stock arrives needs the permissions paying the supplier does.
    if let Some(payment) = &body.payment {
        match payment.source {
            PaymentSource::Drawer | PaymentSource::OwnerPocket => {
                current_user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
                current_user.require_permission(perm::CASH_MOVEMENTS_CREATE_MANAGED)?;
            }
            PaymentSource::Bank | PaymentSource::Card | PaymentSource::Online => {
                current_user.require_permission(perm::EXPENSES_MANAGE)?;
            }
        }
    }
    let recorded_by = RecordedBy {
        name: current_user.resolve_name(&state.db).await,
        user_id: current_user.user_id,
    };
    let recorded = receipt::record_receipt(&state.db, Some(recorded_by), body, device_id.0).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            recorded,
            "Stock received successfully",
        )),
    ))
}
