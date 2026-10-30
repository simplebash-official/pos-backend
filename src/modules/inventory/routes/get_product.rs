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

#[utoipa::path(get, path = "/products/{id}", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a product", body = ApiResponse<Product>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_product(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Product>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let object_id = parse_object_id(&id)?;
    let product = product_for(
        &user,
        service::product::get_product(&state.db, object_id).await?,
    );

    Ok(Json(ApiResponse::success(
        product,
        "Product retrieved successfully",
    )))
}
