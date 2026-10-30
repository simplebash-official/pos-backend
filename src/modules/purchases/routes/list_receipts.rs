use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;

use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::purchase_receipts::{ReceiptListQuery, ReceiptListResponse};

use crate::modules::purchases::service::receipt;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::PURCHASE_RECEIPTS, params(ReceiptListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier receipts, newest first", body = ApiResponse<ReceiptListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_receipts(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<ReceiptListQuery>,
) -> AppResult<Json<ApiResponse<ReceiptListResponse>>> {
    let response = receipt::list_receipts(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Receipts retrieved successfully",
    )))
}
