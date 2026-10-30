use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::supplier_products::{
    BulkReplaceLinksRequest, SupplierProductLink, SupplierProductLinkQuery,
    SupplierProductListResponse, UpsertSupplierProductLinkRequest,
};
use crate::modules::supplier_products::service::link;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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

#[utoipa::path(get, path = "/", tag = modules::SUPPLIER_PRODUCTS, params(SupplierProductLinkQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Links for a supplier and/or product, or full collection paginated", body = ApiResponse<SupplierProductListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_links(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<SupplierProductLinkQuery>,
) -> AppResult<Json<ApiResponse<SupplierProductListResponse>>> {
    let response = link::list_links(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Supplier-product links retrieved successfully",
    )))
}

#[utoipa::path(put, path = "/bulk/{supplierKey}", tag = modules::SUPPLIER_PRODUCTS,
    params(("supplierKey" = String, Path, description = "Supplier key")),
    request_body = BulkReplaceLinksRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier's product links replaced", body = ApiResponse<Vec<SupplierProductLink>>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or one of the products not found", body = ErrorResponse),
    )
)]
pub(super) async fn replace_links_for_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(supplier_key): Path<String>,
    Json(body): Json<BulkReplaceLinksRequest>,
) -> AppResult<Json<ApiResponse<Vec<SupplierProductLink>>>> {
    let links = link::replace_links_for_supplier(&state.db, supplier_key, body).await?;

    Ok(Json(ApiResponse::success(
        links,
        "Supplier product links updated",
    )))
}

/// Routes of the supplier products module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_links, upsert_link))
        .routes(routes!(delete_link))
        .routes(routes!(replace_links_for_supplier))
}

#[utoipa::path(post, path = "/", tag = modules::SUPPLIER_PRODUCTS, request_body = UpsertSupplierProductLinkRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Link created or updated", body = ApiResponse<SupplierProductLink>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or product not found", body = ErrorResponse),
    )
)]
pub(super) async fn upsert_link(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpsertSupplierProductLinkRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<SupplierProductLink>>)> {
    let link = link::upsert_link(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(link, "Product linked to supplier")),
    ))
}
