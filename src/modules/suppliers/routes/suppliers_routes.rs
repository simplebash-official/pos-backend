use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::{AdminUser, CurrentUser};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::utils::parse_object_id as parse_mongo_id;
use crate::domain::suppliers::{
    CreateSupplierRequest, DeleteSuppliersRequest, DeleteSuppliersResponse, Supplier,
    SupplierCategoriesResponse, SupplierListQuery, SupplierStats, SuppliersResponse,
    UpdateSupplierRequest,
};
use crate::modules::suppliers::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(post, path = "/", tag = modules::SUPPLIERS, request_body = CreateSupplierRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Supplier created", body = ApiResponse<Supplier>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Missing the suppliers:create permission", body = ErrorResponse),
    )
)]
pub(super) async fn create_supplier(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateSupplierRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Supplier>>)> {
    current_user.require_permission(perm::SUPPLIERS_CREATE)?;
    let supplier = service::create_supplier(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            supplier,
            "Supplier created successfully",
        )),
    ))
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
pub(super) async fn delete_supplier(
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
pub(super) async fn delete_suppliers_batch(
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
pub(super) async fn get_supplier(
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

#[utoipa::path(get, path = "/categories", tag = modules::SUPPLIERS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Distinct supplied-category tags in use", body = ApiResponse<SupplierCategoriesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn get_supplier_categories(
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
pub(super) async fn get_supplier_stats(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<SupplierStats>>> {
    let stats = service::get_supplier_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Supplier stats retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/", tag = modules::SUPPLIERS, params(SupplierListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List suppliers", body = ApiResponse<SuppliersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_suppliers(
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

/// Parses a `Path<String>` id param — this is the only place in the file
/// that turns a raw path segment into an `ObjectId`; every handler that
/// needs one calls this before delegating to `service::*`.
pub(super) fn parse_object_id(id: &str) -> AppResult<ObjectId> {
    parse_mongo_id(id, "Supplier")
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
pub(super) async fn replace_supplier(
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

/// Routes of the suppliers module.
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
pub(super) async fn update_supplier(
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
