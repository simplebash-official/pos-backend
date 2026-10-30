use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::supplier_products::service::link;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(delete, path = "/{supplierKey}/{productKey}", tag = modules::SUPPLIER_PRODUCTS,
    params(
        ("supplierKey" = String, Path, description = "Supplier key"),
        ("productKey" = String, Path, description = "Product key"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Link removed"),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Link not found", body = ErrorResponse),
    )
)]
pub(super) async fn delete_link(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((supplier_key, product_key)): Path<(String, String)>,
) -> AppResult<Json<ApiResponse<()>>> {
    link::delete_link(&state.db, &supplier_key, &product_key).await?;

    Ok(Json(ApiResponse::message("Product unlinked from supplier")))
}
