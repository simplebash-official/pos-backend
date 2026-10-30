use super::parse_object_id::parse_object_id;
use super::product_for::product_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{Product, UpdateProductRequest};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(put, path = "/products/{id}", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    request_body = UpdateProductRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Product updated", body = ApiResponse<Product>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
pub(super) async fn update_product(
    user: CurrentUser,
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdateProductRequest>,
) -> AppResult<Json<ApiResponse<Product>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let object_id = parse_object_id(&id)?;
    let product = product_for(
        &user,
        service::product::update_product(&state.db, object_id, body, if_match.0, device_id.0)
            .await?,
    );

    Ok(Json(ApiResponse::success(
        product,
        "Product updated successfully",
    )))
}
