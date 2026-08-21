// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there. Unlike most other modules, there's no
// placeholder `GET /` status route here — the collection root (`GET /`,
// nested at `/api/suppliers`) is itself the real "list suppliers" endpoint
// from day one, so there's no unused path left for a status ping. Every
// route — reads included — requires `AdminUser`: supplier data (pricing,
// contacts) is treated as business-sensitive, not general shop-floor data.
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use mongodb::bson::oid::ObjectId;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::AdminUser,
        response::{ApiResponse, ErrorResponse},
        utils::parse_object_id as parse_mongo_id,
    },
    domain::suppliers::{
        CreateSupplierRequest, DeleteSuppliersRequest, DeleteSuppliersResponse, Supplier,
        SupplierCategoriesResponse, SupplierListQuery, SupplierStats, SuppliersResponse,
        UpdateSupplierRequest,
    },
    modules::suppliers::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Suppliers
        .routes(routes!(list_suppliers, create_supplier))
        .routes(routes!(
            get_supplier,
            replace_supplier,
            update_supplier,
            delete_supplier
        ))
        .routes(routes!(delete_suppliers_batch))
        .routes(routes!(get_supplier_categories))
        .routes(routes!(get_supplier_stats))
}

// ============================================================================
// Helpers
// ============================================================================

/// Parses a `Path<String>` id param — this is the only place in the file
/// that turns a raw path segment into an `ObjectId`; every handler that
/// needs one calls this before delegating to `service::*`.
fn parse_object_id(id: &str) -> AppResult<ObjectId> {
    parse_mongo_id(id, "Supplier")
}

// ============================================================================
// Suppliers
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::SUPPLIERS, params(SupplierListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List suppliers", body = ApiResponse<SuppliersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
async fn list_suppliers(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<SupplierListQuery>,
) -> AppResult<Json<ApiResponse<SuppliersResponse>>> {
    let response = service::list_suppliers(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Suppliers retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/", tag = modules::SUPPLIERS, request_body = CreateSupplierRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Supplier created", body = ApiResponse<Supplier>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
async fn create_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateSupplierRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Supplier>>)> {
    let supplier = service::create_supplier(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            supplier,
            "Supplier created successfully",
        )),
    ))
}

#[utoipa::path(get, path = "/{id}", tag = modules::SUPPLIERS,
    params(("id" = String, Path, description = "Supplier id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a supplier", body = ApiResponse<Supplier>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier not found", body = ErrorResponse),
    )
)]
async fn get_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Supplier>>> {
    let object_id = parse_object_id(&id)?;
    let supplier = service::get_supplier(&state.db, object_id).await?;

    Ok(Json(ApiResponse::success(
        supplier,
        "Supplier retrieved successfully",
    )))
}

#[utoipa::path(put, path = "/{id}", tag = modules::SUPPLIERS,
    params(("id" = String, Path, description = "Supplier id")),
    request_body = CreateSupplierRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier replaced", body = ApiResponse<Supplier>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
async fn replace_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<CreateSupplierRequest>,
) -> AppResult<Json<ApiResponse<Supplier>>> {
    let object_id = parse_object_id(&id)?;
    let supplier =
        service::replace_supplier(&state.db, object_id, body, if_match.0, device_id.0).await?;

    Ok(Json(ApiResponse::success(
        supplier,
        "Supplier updated successfully",
    )))
}

#[utoipa::path(patch, path = "/{id}", tag = modules::SUPPLIERS,
    params(("id" = String, Path, description = "Supplier id")),
    request_body = UpdateSupplierRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier updated", body = ApiResponse<Supplier>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
async fn update_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdateSupplierRequest>,
) -> AppResult<Json<ApiResponse<Supplier>>> {
    let object_id = parse_object_id(&id)?;
    let supplier =
        service::update_supplier(&state.db, object_id, body, if_match.0, device_id.0).await?;

    Ok(Json(ApiResponse::success(
        supplier,
        "Supplier updated successfully",
    )))
}

#[utoipa::path(delete, path = "/{id}", tag = modules::SUPPLIERS,
    params(("id" = String, Path, description = "Supplier id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier deleted", body = ApiResponse<Supplier>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier not found", body = ErrorResponse),
        (status = 409, description = "Supplier is still referenced by purchase history or version conflict", body = ErrorResponse),
    )
)]
async fn delete_supplier(
    _admin: AdminUser,
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Supplier>>> {
    let object_id = parse_object_id(&id)?;
    let supplier = service::delete_supplier(&state.db, object_id, if_match.0, device_id.0).await?;

    Ok(Json(ApiResponse::success(
        supplier,
        "Supplier deleted successfully",
    )))
}

#[utoipa::path(delete, path = "/batch", tag = modules::SUPPLIERS, request_body = DeleteSuppliersRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Suppliers deleted", body = ApiResponse<DeleteSuppliersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
async fn delete_suppliers_batch(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<DeleteSuppliersRequest>,
) -> AppResult<Json<ApiResponse<DeleteSuppliersResponse>>> {
    let deleted_count = service::delete_suppliers(&state.db, body.ids).await?;

    Ok(Json(ApiResponse::success(
        DeleteSuppliersResponse { deleted_count },
        format!("{deleted_count} suppliers deleted successfully"),
    )))
}

#[utoipa::path(get, path = "/categories", tag = modules::SUPPLIERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Distinct supplied-category tags in use", body = ApiResponse<SupplierCategoriesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
async fn get_supplier_categories(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SupplierCategoriesResponse>>> {
    let response = service::get_supplier_categories(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Supplier categories retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/stats", tag = modules::SUPPLIERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Suppliers screen KPI cards: total suppliers, supply category count, and direct-contact count", body = ApiResponse<SupplierStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
async fn get_supplier_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SupplierStats>>> {
    let stats = service::get_supplier_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Supplier stats retrieved successfully",
    )))
}
