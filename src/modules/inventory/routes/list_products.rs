use super::product_for::product_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{ProductListQuery, ProductListResponse};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/products", tag = modules::INVENTORY, params(ProductListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List products", body = ApiResponse<ProductListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn list_products(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ProductListQuery>,
) -> AppResult<Json<ApiResponse<ProductListResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let mut response = service::product::list_products(&state.db, query).await?;
    response.items = response
        .items
        .into_iter()
        .map(|product| product_for(&user, product))
        .collect();

    Ok(Json(ApiResponse::success(
        response,
        "Products retrieved successfully",
    )))
}
