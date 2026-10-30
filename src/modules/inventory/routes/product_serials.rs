use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{ProductSerialListQuery, ProductSerialListResponse};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, Query, State};

#[utoipa::path(get, path = "/products/{key}/serials", tag = modules::INVENTORY,
    params(
        ("key" = String, Path, description = "Product key"),
        ProductSerialListQuery,
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Serialized units for a product, optionally filtered by status", body = ApiResponse<ProductSerialListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn product_serials(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<ProductSerialListQuery>,
) -> AppResult<Json<ApiResponse<ProductSerialListResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let items =
        service::product_serial::list_serials_for_product(&state.db, &key, query.status).await?;

    Ok(Json(ApiResponse::success(
        ProductSerialListResponse { items },
        "Product serials retrieved successfully",
    )))
}
