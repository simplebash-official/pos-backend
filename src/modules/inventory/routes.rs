// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — so individual handlers aren't commented
// beyond that pattern; the business logic they call into lives in
// `service/` and is commented there.

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
        utils::{module_status_response, parse_object_id as parse_mongo_id},
    },
    domain::{
        ModuleStatusResponse,
        inventory::{
            AddSubcategoryRequest, CategoriesResponse, CategoryInfo, CreateCategoryRequest,
            CreateProductRequest, DeleteProductsRequest, DeleteProductsResponse,
            InventoryOverviewQuery, InventoryOverviewResponse, LowStockResponse, Product,
            ProductListQuery, ProductListResponse, StockAdjustmentRequest, StockAdjustmentResponse,
            StockMovementListQuery, StockMovementListResponse, StockMovementsResponse,
            SubcategoriesResponse, UpdateCategoryRequest, UpdateProductRequest,
            ValidCategoriesResponse,
        },
    },
    modules::inventory::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    // Each `routes!()` call registers one axum path — handlers only share a
    // call when they're different HTTP methods on the *same* path (e.g.
    // GET/PUT/DELETE all on "/products/{id}"). Mixing handlers with
    // different paths into one call would fold them onto a single
    // `MethodRouter` and panic on the first repeated method.
    OpenApiRouter::new()
        // Module
        .routes(routes!(status))
        // Overview
        .routes(routes!(inventory_overview))
        // Products
        .routes(routes!(list_products, create_product, delete_products))
        .routes(routes!(get_product, update_product, delete_product))
        // Stock
        .routes(routes!(list_all_stock_movements))
        .routes(routes!(adjust_stock))
        .routes(routes!(low_stock))
        .routes(routes!(product_movements))
        // Categories
        .routes(routes!(list_categories, create_category))
        .routes(routes!(update_category, delete_category))
        .routes(routes!(get_valid_categories))
        // Subcategories
        .routes(routes!(get_category_subcategories, add_subcategory))
        .routes(routes!(remove_subcategory))
}

// ============================================================================
// Helpers
// ============================================================================

/// Parses a `Path<String>` id param — this is the only place in the file
/// that turns a raw path segment into an `ObjectId`; every handler that
/// needs one calls this before delegating to `service::*`.
fn parse_object_id(id: &str) -> AppResult<ObjectId> {
    parse_mongo_id(id, "Product")
}

// ============================================================================
// Module status
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::INVENTORY, responses(
    (status = 200, description = "Inventory module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::INVENTORY)
}

// ============================================================================
// Overview
// ============================================================================

#[utoipa::path(get, path = "/overview", tag = modules::INVENTORY, params(InventoryOverviewQuery), responses(
    (status = 200, description = "Get inventory overview with metrics, category hierarchy, subcategory data tables, search, and filtration", body = ApiResponse<InventoryOverviewResponse>)
))]
async fn inventory_overview(
    State(state): State<AppState>,
    Query(query): Query<InventoryOverviewQuery>,
) -> AppResult<Json<ApiResponse<InventoryOverviewResponse>>> {
    let response = service::overview::get_inventory_overview(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Inventory overview retrieved successfully",
    )))
}

// ============================================================================
// Products
// ============================================================================

#[utoipa::path(get, path = "/products", tag = modules::INVENTORY, params(ProductListQuery), responses(
    (status = 200, description = "List products", body = ApiResponse<ProductListResponse>)
))]
async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductListQuery>,
) -> AppResult<Json<ApiResponse<ProductListResponse>>> {
    let response = service::product::list_products(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Products retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/products/{id}", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    responses(
        (status = 200, description = "Get a product", body = ApiResponse<Product>),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Product>>> {
    let object_id = parse_object_id(&id)?;
    let product = service::product::get_product(&state.db, object_id).await?;

    Ok(Json(ApiResponse::success(
        product,
        "Product retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/products", tag = modules::INVENTORY, request_body = CreateProductRequest,
    responses(
        (status = 201, description = "Product created", body = ApiResponse<Product>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 409, description = "SKU already exists", body = ErrorResponse),
    )
)]
async fn create_product(
    State(state): State<AppState>,
    Json(body): Json<CreateProductRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Product>>)> {
    let product = service::product::create_product(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            product,
            "Product created successfully",
        )),
    ))
}

#[utoipa::path(put, path = "/products/{id}", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    request_body = UpdateProductRequest,
    responses(
        (status = 200, description = "Product updated", body = ApiResponse<Product>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
async fn update_product(
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdateProductRequest>,
) -> AppResult<Json<ApiResponse<Product>>> {
    let object_id = parse_object_id(&id)?;
    let product =
        service::product::update_product(&state.db, object_id, body, if_match.0, device_id.0)
            .await?;

    Ok(Json(ApiResponse::success(
        product,
        "Product updated successfully",
    )))
}

#[utoipa::path(delete, path = "/products/{id}", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    responses(
        (status = 200, description = "Product deleted", body = ApiResponse<Product>),
        (status = 404, description = "Product not found", body = ErrorResponse),
        (status = 409, description = "Version conflict", body = ErrorResponse),
    )
)]
async fn delete_product(
    State(state): State<AppState>,
    if_match: crate::core::middleware::sync_headers::IfMatch,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Product>>> {
    let object_id = parse_object_id(&id)?;
    let product =
        service::product::delete_product(&state.db, object_id, if_match.0, device_id.0).await?;

    Ok(Json(ApiResponse::success(
        product,
        "Product deleted successfully",
    )))
}

#[utoipa::path(delete, path = "/products", tag = modules::INVENTORY, request_body = DeleteProductsRequest,
    responses(
        (status = 200, description = "Products deleted", body = ApiResponse<DeleteProductsResponse>),
    )
)]
async fn delete_products(
    State(state): State<AppState>,
    Json(body): Json<DeleteProductsRequest>,
) -> AppResult<Json<ApiResponse<DeleteProductsResponse>>> {
    let deleted_count = service::product::delete_products(&state.db, body.product_ids).await?;

    Ok(Json(ApiResponse::success(
        DeleteProductsResponse { deleted_count },
        format!("{deleted_count} products deleted successfully"),
    )))
}

// ============================================================================
// Stock
// ============================================================================

#[utoipa::path(patch, path = "/products/{id}/stock", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    request_body = StockAdjustmentRequest,
    responses(
        (status = 200, description = "Stock adjusted", body = ApiResponse<StockAdjustmentResponse>),
        (status = 400, description = "Invalid stock adjustment", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
async fn adjust_stock(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StockAdjustmentRequest>,
) -> AppResult<Json<ApiResponse<StockAdjustmentResponse>>> {
    let object_id = parse_object_id(&id)?;
    let response = service::stock::adjust_stock(&state.db, object_id, body).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Stock adjusted successfully",
    )))
}

#[utoipa::path(get, path = "/products/low-stock", tag = modules::INVENTORY,
    responses(
        (status = 200, description = "Products at or below their minimum stock threshold", body = ApiResponse<LowStockResponse>),
    )
)]
async fn low_stock(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<LowStockResponse>>> {
    let response = service::stock::low_stock(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Low stock products retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/products/{id}/movements", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    responses(
        (status = 200, description = "Stock movement history for a product", body = ApiResponse<StockMovementsResponse>),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
async fn product_movements(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<StockMovementsResponse>>> {
    let object_id = parse_object_id(&id)?;
    let response = service::stock::product_movements(&state.db, object_id).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Stock movements retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/stock-movements", tag = modules::INVENTORY, params(StockMovementListQuery),
    responses(
        (status = 200, description = "List all stock movements with pagination", body = ApiResponse<StockMovementListResponse>),
    )
)]
async fn list_all_stock_movements(
    State(state): State<AppState>,
    Query(query): Query<StockMovementListQuery>,
) -> AppResult<Json<ApiResponse<StockMovementListResponse>>> {
    let response = service::stock::list_stock_movements(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Stock movements retrieved successfully",
    )))
}

// ============================================================================
// Categories
// ============================================================================

#[utoipa::path(get, path = "/categories", tag = modules::INVENTORY,
    responses(
        (status = 200, description = "List categories with subcategories", body = ApiResponse<CategoriesResponse>),
    )
)]
async fn list_categories(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<CategoriesResponse>>> {
    let response = service::category::list_categories(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Categories retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/categories", tag = modules::INVENTORY, request_body = CreateCategoryRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Category created", body = ApiResponse<CategoryInfo>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 409, description = "Category already exists", body = ErrorResponse),
    )
)]
async fn create_category(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateCategoryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CategoryInfo>>)> {
    let category = service::category::create_category(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            category,
            "Category created successfully",
        )),
    ))
}

#[utoipa::path(put, path = "/categories/{categoryKey}", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    request_body = UpdateCategoryRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Category updated", body = ApiResponse<CategoryInfo>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "New name already in use", body = ErrorResponse),
    )
)]
async fn update_category(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(category_key): Path<String>,
    Json(body): Json<UpdateCategoryRequest>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let updated = service::category::update_category(&state.db, category_key, body).await?;

    Ok(Json(ApiResponse::success(
        updated,
        "Category updated successfully",
    )))
}

#[utoipa::path(delete, path = "/categories/{categoryKey}", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Category deleted", body = ApiResponse<CategoryInfo>),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "Category is still referenced by products", body = ErrorResponse),
    )
)]
async fn delete_category(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(category_key): Path<String>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let deleted = service::category::delete_category(&state.db, category_key).await?;

    Ok(Json(ApiResponse::success(
        deleted,
        "Category deleted successfully",
    )))
}

#[utoipa::path(get, path = "/categories/valid", tag = modules::INVENTORY,
    responses(
        (status = 200, description = "Valid category/subcategory validation metadata", body = ApiResponse<ValidCategoriesResponse>),
    )
)]
async fn get_valid_categories(
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<ValidCategoriesResponse>>> {
    let response = service::category::get_valid_categories(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Valid categories retrieved successfully",
    )))
}

// ============================================================================
// Subcategories
// ============================================================================

#[utoipa::path(get, path = "/categories/{categoryKey}/subcategories", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    responses(
        (status = 200, description = "Subcategories for a category", body = ApiResponse<SubcategoriesResponse>),
        (status = 404, description = "Category not found", body = ErrorResponse),
    )
)]
async fn get_category_subcategories(
    State(state): State<AppState>,
    Path(category_key): Path<String>,
) -> AppResult<Json<ApiResponse<SubcategoriesResponse>>> {
    let response = service::category::get_category_subcategories(&state.db, category_key).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Subcategories retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/categories/{categoryKey}/subcategories", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    request_body = AddSubcategoryRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Subcategory added", body = ApiResponse<CategoryInfo>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "Subcategory already exists", body = ErrorResponse),
    )
)]
async fn add_subcategory(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(category_key): Path<String>,
    Json(body): Json<AddSubcategoryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CategoryInfo>>)> {
    let updated = service::category::add_subcategory(&state.db, category_key, body.name).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            updated,
            "Subcategory added successfully",
        )),
    ))
}

#[utoipa::path(delete, path = "/categories/{categoryKey}/subcategories/{subcategoryKey}", tag = modules::INVENTORY,
    params(
        ("categoryKey" = String, Path, description = "Category key"),
        ("subcategoryKey" = String, Path, description = "Subcategory key"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Subcategory removed", body = ApiResponse<CategoryInfo>),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category or subcategory not found", body = ErrorResponse),
        (status = 409, description = "Subcategory is still referenced by products", body = ErrorResponse),
    )
)]
async fn remove_subcategory(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((category_key, subcategory_key)): Path<(String, String)>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let updated =
        service::category::remove_subcategory(&state.db, category_key, subcategory_key).await?;

    Ok(Json(ApiResponse::success(
        updated,
        "Subcategory removed successfully",
    )))
}
