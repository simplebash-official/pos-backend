use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::suppliers::SupplierCategoriesResponse;
use crate::modules::suppliers::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/categories", tag = modules::SUPPLIERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Distinct supplied-category tags in use", body = ApiResponse<SupplierCategoriesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn get_supplier_categories(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SupplierCategoriesResponse>>> {
    let response = service::get_supplier_categories(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Supplier categories retrieved successfully",
    )))
}
