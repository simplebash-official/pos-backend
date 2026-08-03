use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use futures_util::TryStreamExt;
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{codes, modules, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        middleware::auth::AdminUser,
        response::{ApiResponse, ErrorResponse},
        utils::{
            build_bson_regex, calculate_pagination, module_status_response,
            parse_object_id as parse_mongo_id,
        },
    },
    domain::{
        ModuleStatusResponse,
        inventory::{
            AddSubcategoryRequest, CategoriesResponse, CategoryInfo, CreateCategoryRequest,
            CreateProductRequest, DeleteProductsRequest, DeleteProductsResponse, LowStockItem,
            LowStockResponse, PaginationMeta, Product, ProductListQuery, ProductListResponse,
            StockAdjustmentRequest, StockAdjustmentResponse, StockMovementType,
            StockMovementsResponse, SubcategoriesResponse, UpdateCategoryRequest,
            UpdateProductRequest, ValidCategoriesResponse,
        },
    },
    modules::inventory::model::{CategoryDocument, ProductDocument, StockMovementDocument},
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
        // Products
        .routes(routes!(list_products, create_product, delete_products))
        .routes(routes!(get_product, update_product, delete_product))
        // Stock
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

fn parse_object_id(id: &str) -> AppResult<ObjectId> {
    parse_mongo_id(id, "Product")
}

/// Maps a client-supplied `sortBy` value to the corresponding document field.
/// Whitelisted rather than passed straight through, since sort keys end up
/// directly in a Mongo `sort` document.
fn sort_field_for(sort_by: Option<&str>) -> &'static str {
    match sort_by {
        Some("name") => "name",
        Some("sku") => "sku",
        Some("category") => "category",
        Some("subcategory") => "subcategory",
        Some("costPriceCents") => "cost_price_cents",
        Some("sellingPriceCents") => "selling_price_cents",
        Some("stockQuantity") => "stock_quantity",
        Some("minStockThreshold") => "min_stock_threshold",
        _ => "updated_at",
    }
}

fn validate_product_numbers(
    selling_price_cents: i64,
    cost_price_cents: i64,
    stock_quantity: i64,
    min_stock_threshold: i64,
) -> AppResult<()> {
    if selling_price_cents <= 0 {
        return Err(AppError::validation("Selling price must be greater than 0"));
    }
    if cost_price_cents < 0 {
        return Err(AppError::validation("Cost price cannot be negative"));
    }
    if stock_quantity < 0 {
        return Err(AppError::validation("Stock quantity cannot be negative"));
    }
    if min_stock_threshold < 0 {
        return Err(AppError::validation(
            "Minimum stock threshold cannot be negative",
        ));
    }
    Ok(())
}

/// Checks `subcategory` is one of the subcategories stored for `category` in
/// the `categories` collection — the DB-backed single source of truth for
/// both the category API and this validation (see
/// `modules::inventory::categories` for the seed data).
async fn ensure_valid_category(db: &Database, category: &str, subcategory: &str) -> AppResult<()> {
    let exists = db
        .collection::<CategoryDocument>("categories")
        .find_one(doc! { "name": category, "subcategories": subcategory })
        .await?
        .is_some();

    if !exists {
        return Err(AppError::validation(format!(
            "'{subcategory}' is not a valid subcategory of '{category}'"
        )));
    }

    Ok(())
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
// Products
// ============================================================================

#[utoipa::path(get, path = "/products", tag = modules::INVENTORY, params(ProductListQuery), responses(
    (status = 200, description = "List products", body = ApiResponse<ProductListResponse>)
))]
async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductListQuery>,
) -> AppResult<Json<ApiResponse<ProductListResponse>>> {
    let collection = state.db.collection::<ProductDocument>("products");

    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(category) = query.category.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "category": category });
    }
    if let Some(subcategory) = query.subcategory.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "subcategory": subcategory });
    }
    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = build_bson_regex(&search);
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "sku": { "$regex": pattern.clone() } },
                { "barcode": { "$regex": pattern.clone() } },
                { "category": { "$regex": pattern.clone() } },
                { "subcategory": { "$regex": pattern } },
            ]
        });
    }
    if query.low_stock == Some(true) {
        and_clauses.push(doc! {
            "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] }
        });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 200);

    let sort_field = sort_field_for(query.sort_by.as_deref());
    let sort_order = if query.sort_order.as_deref() == Some("asc") {
        1
    } else {
        -1
    };

    let total = collection.count_documents(filter.clone()).await?;

    let mut cursor = collection
        .find(filter)
        .sort(doc! { sort_field: sort_order })
        .skip(skip)
        .limit(limit as i64)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document.into_product());
    }

    let pagination = PaginationMeta {
        page,
        limit,
        total,
        total_pages: total.div_ceil(limit),
    };

    Ok(Json(ApiResponse::success(
        ProductListResponse { items, pagination },
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
    let collection = state.db.collection::<ProductDocument>("products");

    let document = collection
        .find_one(doc! { "_id": object_id })
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

    Ok(Json(ApiResponse::success(
        document.into_product(),
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
    if body.sku.trim().is_empty() {
        return Err(AppError::validation("SKU is required"));
    }
    if body.name.trim().is_empty() {
        return Err(AppError::validation("Product name is required"));
    }
    validate_product_numbers(
        body.selling_price_cents,
        body.cost_price_cents,
        body.stock_quantity,
        body.min_stock_threshold,
    )?;
    ensure_valid_category(&state.db, &body.category, &body.subcategory).await?;

    let collection = state.db.collection::<ProductDocument>("products");

    if collection
        .find_one(doc! { "sku": &body.sku })
        .await?
        .is_some()
    {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::SKU_ALREADY_EXISTS,
            format!("A product with SKU '{}' already exists", body.sku),
        ));
    }

    let mut document = ProductDocument {
        id: None,
        key: generate_id(prefixes::PRODUCT),
        sku: body.sku,
        barcode: body.barcode,
        name: body.name,
        category: body.category,
        subcategory: body.subcategory,
        cost_price_cents: body.cost_price_cents,
        selling_price_cents: body.selling_price_cents,
        stock_quantity: body.stock_quantity,
        min_stock_threshold: body.min_stock_threshold,
        updated_at: BsonDateTime::now(),
    };

    let insert_result = collection.insert_one(&document).await?;
    document.id = Some(
        insert_result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            document.into_product(),
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
    )
)]
async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateProductRequest>,
) -> AppResult<Json<ApiResponse<Product>>> {
    let object_id = parse_object_id(&id)?;
    let collection = state.db.collection::<ProductDocument>("products");

    let existing = collection
        .find_one(doc! { "_id": object_id })
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

    if let Some(name) = &body.name
        && name.trim().is_empty()
    {
        return Err(AppError::validation("Product name cannot be empty"));
    }

    let category = body.category.unwrap_or(existing.category);
    let subcategory = body.subcategory.unwrap_or(existing.subcategory);
    let selling_price_cents = body
        .selling_price_cents
        .unwrap_or(existing.selling_price_cents);
    let cost_price_cents = body.cost_price_cents.unwrap_or(existing.cost_price_cents);
    let stock_quantity = body.stock_quantity.unwrap_or(existing.stock_quantity);
    let min_stock_threshold = body
        .min_stock_threshold
        .unwrap_or(existing.min_stock_threshold);

    validate_product_numbers(
        selling_price_cents,
        cost_price_cents,
        stock_quantity,
        min_stock_threshold,
    )?;
    ensure_valid_category(&state.db, &category, &subcategory).await?;

    let mut set_doc = doc! {
        "category": &category,
        "subcategory": &subcategory,
        "selling_price_cents": selling_price_cents,
        "cost_price_cents": cost_price_cents,
        "stock_quantity": stock_quantity,
        "min_stock_threshold": min_stock_threshold,
        "updated_at": BsonDateTime::now(),
    };
    if let Some(name) = body.name {
        set_doc.insert("name", name);
    }
    if let Some(barcode) = body.barcode {
        set_doc.insert("barcode", barcode);
    }

    let updated = collection
        .find_one_and_update(doc! { "_id": object_id }, doc! { "$set": set_doc })
        .return_document(ReturnDocument::After)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

    Ok(Json(ApiResponse::success(
        updated.into_product(),
        "Product updated successfully",
    )))
}

#[utoipa::path(delete, path = "/products/{id}", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    responses(
        (status = 200, description = "Product deleted", body = ApiResponse<Product>),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Product>>> {
    let object_id = parse_object_id(&id)?;
    let collection = state.db.collection::<ProductDocument>("products");

    let deleted = collection
        .find_one_and_delete(doc! { "_id": object_id })
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

    Ok(Json(ApiResponse::success(
        deleted.into_product(),
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
    let object_ids: Vec<ObjectId> = body
        .product_ids
        .iter()
        .filter_map(|id| ObjectId::parse_str(id).ok())
        .collect();

    let collection = state.db.collection::<ProductDocument>("products");
    let deleted_count = if object_ids.is_empty() {
        0
    } else {
        collection
            .delete_many(doc! { "_id": { "$in": object_ids } })
            .await?
            .deleted_count
    };

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
    let collection = state.db.collection::<ProductDocument>("products");

    let existing = collection
        .find_one(doc! { "_id": object_id })
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

    let previous_stock_quantity = existing.stock_quantity;
    let new_quantity = previous_stock_quantity + body.delta;

    if new_quantity < 0 {
        return Err(AppError::validation_with_code(
            format!(
                "Requested delta {} would result in negative stock (current stock: {previous_stock_quantity})",
                body.delta
            ),
            "INSUFFICIENT_STOCK",
        ));
    }

    let now = BsonDateTime::now();
    let updated = collection
        .find_one_and_update(
            doc! { "_id": object_id },
            doc! {
                "$set": {
                    "stock_quantity": new_quantity,
                    "updated_at": now,
                }
            },
        )
        .return_document(ReturnDocument::After)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Product not found", "PRODUCT_NOT_FOUND"))?;

    state
        .db
        .collection::<StockMovementDocument>("stock_movements")
        .insert_one(StockMovementDocument {
            id: None,
            key: generate_id(prefixes::STOCK_MOVEMENT),
            product_id: object_id,
            quantity_delta: body.delta,
            movement_type: StockMovementType::ManualAdjustment,
            reference_id: None,
            note: body.reason,
            created_at: now,
        })
        .await?;

    Ok(Json(ApiResponse::success(
        StockAdjustmentResponse {
            id: updated
                .id
                .expect("persisted product document must have an id")
                .to_hex(),
            key: updated.key,
            sku: updated.sku,
            name: updated.name,
            stock_quantity: updated.stock_quantity,
            previous_stock_quantity,
            delta: body.delta,
            updated_at: updated.updated_at.to_chrono(),
        },
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
    let collection = state.db.collection::<ProductDocument>("products");
    let mut cursor = collection
        .find(doc! { "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(LowStockItem {
            id: document
                .id
                .expect("persisted product document must have an id")
                .to_hex(),
            key: document.key,
            sku: document.sku,
            name: document.name,
            stock_quantity: document.stock_quantity,
            min_stock_threshold: document.min_stock_threshold,
            deficit: document.min_stock_threshold - document.stock_quantity,
        });
    }

    let total = items.len() as u64;

    Ok(Json(ApiResponse::success(
        LowStockResponse { items, total },
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

    let products = state.db.collection::<ProductDocument>("products");
    if products
        .find_one(doc! { "_id": object_id })
        .await?
        .is_none()
    {
        return Err(AppError::not_found_with_code(
            "Product not found",
            "PRODUCT_NOT_FOUND",
        ));
    }

    let mut cursor = state
        .db
        .collection::<StockMovementDocument>("stock_movements")
        .find(doc! { "product_id": object_id })
        .sort(doc! { "created_at": 1 })
        .await?;

    let mut movements = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        movements.push(document.into_stock_movement());
    }

    Ok(Json(ApiResponse::success(
        StockMovementsResponse { movements },
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
    let mut cursor = state
        .db
        .collection::<CategoryDocument>("categories")
        .find(doc! {})
        .sort(doc! { "name": 1 })
        .await?;

    let mut categories = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        categories.push(document.into_category_info());
    }

    Ok(Json(ApiResponse::success(
        CategoriesResponse { categories },
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
    if body.name.trim().is_empty() {
        return Err(AppError::validation("Category name is required"));
    }
    if body.icon.trim().is_empty() {
        return Err(AppError::validation("Icon is required"));
    }
    if body.color.trim().is_empty() {
        return Err(AppError::validation("Color is required"));
    }

    let collection = state.db.collection::<CategoryDocument>("categories");
    if collection
        .find_one(doc! { "name": &body.name })
        .await?
        .is_some()
    {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::CATEGORY_ALREADY_EXISTS,
            format!("A category named '{}' already exists", body.name),
        ));
    }

    let document = CategoryDocument {
        id: None,
        key: generate_id(prefixes::CATEGORY),
        name: body.name,
        icon: body.icon,
        color: body.color,
        subcategories: body.subcategories,
    };
    collection.insert_one(&document).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            document.into_category_info(),
            "Category created successfully",
        )),
    ))
}

#[utoipa::path(put, path = "/categories/{category}", tag = modules::INVENTORY,
    params(("category" = String, Path, description = "Main category name")),
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
    Path(category): Path<String>,
    Json(body): Json<UpdateCategoryRequest>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    if let Some(name) = &body.name
        && name.trim().is_empty()
    {
        return Err(AppError::validation("Category name cannot be empty"));
    }
    if let Some(icon) = &body.icon
        && icon.trim().is_empty()
    {
        return Err(AppError::validation("Icon cannot be empty"));
    }
    if let Some(color) = &body.color
        && color.trim().is_empty()
    {
        return Err(AppError::validation("Color cannot be empty"));
    }

    let collection = state.db.collection::<CategoryDocument>("categories");
    let existing = collection
        .find_one(doc! { "name": &category })
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    let new_name = body.name.unwrap_or_else(|| existing.name.clone());
    if new_name != existing.name
        && collection
            .find_one(doc! { "name": &new_name })
            .await?
            .is_some()
    {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::CATEGORY_ALREADY_EXISTS,
            format!("A category named '{new_name}' already exists"),
        ));
    }

    let icon = body.icon.unwrap_or(existing.icon);
    let color = body.color.unwrap_or(existing.color);

    let updated = collection
        .find_one_and_update(
            doc! { "name": &category },
            doc! { "$set": { "name": &new_name, "icon": icon, "color": color } },
        )
        .return_document(ReturnDocument::After)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    // Products reference categories by name, not id — cascade the rename so
    // they don't silently point at a category that no longer exists.
    let renamed_product_count: u64 = if new_name != category {
        state
            .db
            .collection::<ProductDocument>("products")
            .update_many(
                doc! { "category": &category },
                doc! { "$set": { "category": &new_name } },
            )
            .await?
            .modified_count
    } else {
        0
    };

    let message = if renamed_product_count > 0 {
        format!(
            "Category updated successfully ({renamed_product_count} product(s) renamed to match)"
        )
    } else {
        "Category updated successfully".to_string()
    };

    Ok(Json(ApiResponse::success(
        updated.into_category_info(),
        message,
    )))
}

#[utoipa::path(delete, path = "/categories/{category}", tag = modules::INVENTORY,
    params(("category" = String, Path, description = "Main category name")),
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
    Path(category): Path<String>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let products_using_category = state
        .db
        .collection::<ProductDocument>("products")
        .count_documents(doc! { "category": &category })
        .await?;

    if products_using_category > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::CATEGORY_IN_USE,
            format!("{products_using_category} product(s) still reference category '{category}'"),
        ));
    }

    let deleted = state
        .db
        .collection::<CategoryDocument>("categories")
        .find_one_and_delete(doc! { "name": &category })
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    Ok(Json(ApiResponse::success(
        deleted.into_category_info(),
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
    let mut cursor = state
        .db
        .collection::<CategoryDocument>("categories")
        .find(doc! {})
        .await?;

    let mut valid_categories = Vec::new();
    let mut category_subcategory_map = std::collections::HashMap::new();
    while let Some(document) = cursor.try_next().await? {
        valid_categories.push(document.name.clone());
        category_subcategory_map.insert(document.name, document.subcategories);
    }

    Ok(Json(ApiResponse::success(
        ValidCategoriesResponse {
            valid_categories,
            category_subcategory_map,
        },
        "Valid categories retrieved successfully",
    )))
}

// ============================================================================
// Subcategories
// ============================================================================

#[utoipa::path(get, path = "/categories/{category}/subcategories", tag = modules::INVENTORY,
    params(("category" = String, Path, description = "Main category name")),
    responses(
        (status = 200, description = "Subcategories for a category", body = ApiResponse<SubcategoriesResponse>),
        (status = 404, description = "Category not found", body = ErrorResponse),
    )
)]
async fn get_category_subcategories(
    State(state): State<AppState>,
    Path(category): Path<String>,
) -> AppResult<Json<ApiResponse<SubcategoriesResponse>>> {
    let document = state
        .db
        .collection::<CategoryDocument>("categories")
        .find_one(doc! { "name": &category })
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    Ok(Json(ApiResponse::success(
        SubcategoriesResponse {
            category,
            subcategories: document.subcategories,
        },
        "Subcategories retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/categories/{category}/subcategories", tag = modules::INVENTORY,
    params(("category" = String, Path, description = "Main category name")),
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
    Path(category): Path<String>,
    Json(body): Json<AddSubcategoryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CategoryInfo>>)> {
    if body.subcategory.trim().is_empty() {
        return Err(AppError::validation("Subcategory name is required"));
    }

    let collection = state.db.collection::<CategoryDocument>("categories");
    let existing = collection
        .find_one(doc! { "name": &category })
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    if existing
        .subcategories
        .iter()
        .any(|s| s == &body.subcategory)
    {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::SUBCATEGORY_ALREADY_EXISTS,
            format!("'{}' already exists under '{category}'", body.subcategory),
        ));
    }

    let updated = collection
        .find_one_and_update(
            doc! { "name": &category },
            doc! { "$push": { "subcategories": &body.subcategory } },
        )
        .return_document(ReturnDocument::After)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            updated.into_category_info(),
            "Subcategory added successfully",
        )),
    ))
}

#[utoipa::path(delete, path = "/categories/{category}/subcategories/{subcategory}", tag = modules::INVENTORY,
    params(
        ("category" = String, Path, description = "Main category name"),
        ("subcategory" = String, Path, description = "Subcategory name"),
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
    Path((category, subcategory)): Path<(String, String)>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let collection = state.db.collection::<CategoryDocument>("categories");
    let existing = collection
        .find_one(doc! { "name": &category })
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Category not found", codes::CATEGORY_NOT_FOUND)
        })?;

    if !existing.subcategories.iter().any(|s| s == &subcategory) {
        return Err(AppError::not_found_with_code(
            "Subcategory not found",
            codes::SUBCATEGORY_NOT_FOUND,
        ));
    }

    let products_using_subcategory = state
        .db
        .collection::<ProductDocument>("products")
        .count_documents(doc! { "category": &category, "subcategory": &subcategory })
        .await?;

    if products_using_subcategory > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::SUBCATEGORY_IN_USE,
            format!(
                "{products_using_subcategory} product(s) still reference subcategory '{subcategory}'"
            ),
        ));
    }

    let updated = collection
        .find_one_and_update(
            doc! { "name": &category },
            doc! { "$pull": { "subcategories": &subcategory } },
        )
        .return_document(ReturnDocument::After)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("Category not found", "CATEGORY_NOT_FOUND"))?;

    Ok(Json(ApiResponse::success(
        updated.into_category_info(),
        "Subcategory removed successfully",
    )))
}
