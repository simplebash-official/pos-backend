use super::parse_object_id::parse_object_id;
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

#[utoipa::path(post, path = "/products/{id}/barcode", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A shop barcode was generated for the product", body = ApiResponse<Product>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
        (status = 409, description = "PRODUCT_HAS_BARCODE: the product already has a barcode", body = ErrorResponse),
    )
)]
pub(super) async fn generate_product_barcode(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Product>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let object_id = parse_object_id(&id)?;
    let product = product_for(
        &user,
        service::product::generate_barcode(&state.db, object_id, device_id.0).await?,
    );

    Ok(Json(ApiResponse::success(
        product,
        "Barcode created successfully",
    )))
}
