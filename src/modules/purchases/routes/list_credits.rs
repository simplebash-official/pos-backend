use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::supplier_credits::{SupplierCreditListQuery, SupplierCreditListResponse};

use crate::modules::purchases::service::credit;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::SUPPLIER_CREDITS, params(SupplierCreditListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier credits, newest first", body = ApiResponse<SupplierCreditListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_credits(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<SupplierCreditListQuery>,
) -> AppResult<Json<ApiResponse<SupplierCreditListResponse>>> {
    let response = credit::list_credits(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Credits retrieved successfully",
    )))
}
