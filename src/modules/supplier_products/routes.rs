// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there. No placeholder status route here for
// the same reason as `suppliers`: the collection root is a real endpoint.
// Every route — reads included — requires `AdminUser`, matching `suppliers`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::AdminUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::supplier_products::{
        BulkReplaceLinksRequest, SupplierProductLink, SupplierProductLinkQuery,
        SupplierProductListResponse, UpsertSupplierProductLinkRequest,
    },
    modules::supplier_products::service::link,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_links, upsert_link))
        .routes(routes!(delete_link))
        .routes(routes!(replace_links_for_supplier))
}

// ============================================================================
// Supplier-product links
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::SUPPLIER_PRODUCTS, params(SupplierProductLinkQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Links for a supplier and/or product, or full collection paginated", body = ApiResponse<SupplierProductListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
async fn list_links(
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
async fn upsert_link(
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
async fn delete_link(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((supplier_key, product_key)): Path<(String, String)>,
) -> AppResult<Json<ApiResponse<()>>> {
    link::delete_link(&state.db, &supplier_key, &product_key).await?;

    Ok(Json(ApiResponse::message("Product unlinked from supplier")))
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
async fn replace_links_for_supplier(
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
