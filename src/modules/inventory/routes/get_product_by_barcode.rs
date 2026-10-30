use super::product_for::product_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::Product;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/products/by-barcode/{barcode}", tag = modules::INVENTORY,
    params(("barcode" = String, Path, description = "Exact barcode to look up")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get the product carrying this barcode", body = ApiResponse<Product>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "No product with this barcode", body = ErrorResponse),
    )
)]
pub(super) async fn get_product_by_barcode(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(barcode): Path<String>,
) -> AppResult<Json<ApiResponse<Product>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let product = product_for(
        &user,
        service::product::get_product_by_barcode(&state.db, &barcode).await?,
    );

    Ok(Json(ApiResponse::success(
        product,
        "Product retrieved successfully",
    )))
}
