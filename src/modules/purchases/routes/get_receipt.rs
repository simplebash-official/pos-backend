use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::purchase_receipts::PurchaseReceipt;

use crate::modules::purchases::service::receipt;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/{key}", tag = modules::PURCHASE_RECEIPTS,
    params(("key" = String, Path, description = "Receipt key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "One receipt with its lines", body = ApiResponse<PurchaseReceipt>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Receipt not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_receipt(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<PurchaseReceipt>>> {
    let response = receipt::get_receipt(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Receipt retrieved successfully",
    )))
}
