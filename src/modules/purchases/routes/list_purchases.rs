use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::purchases::{PurchaseListQuery, PurchaseListResponse};

use crate::modules::purchases::service::purchase;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::PURCHASES, params(PurchaseListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Purchase history for a supplier and/or product, or full collection paginated", body = ApiResponse<PurchaseListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_purchases(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<PurchaseListQuery>,
) -> AppResult<Json<ApiResponse<PurchaseListResponse>>> {
    let response = purchase::list_purchases(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Purchases retrieved successfully",
    )))
}
