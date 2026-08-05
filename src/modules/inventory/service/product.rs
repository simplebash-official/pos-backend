// Business rules for product CRUD/listing: price/stock invariants, SKU
// uniqueness, and category/subcategory validation against the `categories`
// collection. Delegates all Mongo access to `repository::product`.

use axum::http::StatusCode;
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::{build_bson_regex, calculate_pagination},
    },
    domain::inventory::{
        CreateProductRequest, PaginationMeta, Product, ProductListQuery, ProductListResponse,
        UpdateProductRequest,
    },
    modules::inventory::{model::ProductDocument, repository},
};

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
/// both the category API and this validation.
async fn ensure_valid_category(db: &Database, category: &str, subcategory: &str) -> AppResult<()> {
    let exists = repository::category::category_has_subcategory(db, category, subcategory).await?;

    if !exists {
        return Err(AppError::validation(format!(
            "'{subcategory}' is not a valid subcategory of '{category}'"
        )));
    }

    Ok(())
}

/// Builds the Mongo filter/sort from query params (category, subcategory,
/// free-text search across name/sku/barcode/category/subcategory, and the
/// low-stock flag) and delegates execution + pagination math to
/// `repository::product::list_products`.
pub(crate) async fn list_products(
    db: &Database,
    query: ProductListQuery,
) -> AppResult<ProductListResponse> {
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

    let (documents, total) = repository::product::list_products(
        db,
        filter,
        doc! { sort_field: sort_order },
        skip,
        limit as i64,
    )
    .await?;

    let items = documents
        .into_iter()
        .map(ProductDocument::into_product)
        .collect();

    let pagination = PaginationMeta {
        page,
        limit,
        total,
        total_pages: total.div_ceil(limit),
    };

    Ok(ProductListResponse { items, pagination })
}

/// Fetch by id, 404ing with the module-specific `PRODUCT_NOT_FOUND` code
/// rather than the generic `NOT_FOUND`.
pub(crate) async fn get_product(db: &Database, id: ObjectId) -> AppResult<Product> {
    let document = repository::product::find_product_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    Ok(document.into_product())
}

/// Validates required fields, price/stock invariants, and category
/// membership, then generates the product's SKU from its category/
/// subcategory (see `service::sku::generate_sku`) — validation runs first
/// so an invalid category never consumes a sequence number.
pub(crate) async fn create_product(
    db: &Database,
    body: CreateProductRequest,
) -> AppResult<Product> {
    if body.name.trim().is_empty() {
        return Err(AppError::validation("Product name is required"));
    }
    validate_product_numbers(
        body.selling_price_cents,
        body.cost_price_cents,
        body.stock_quantity,
        body.min_stock_threshold,
    )?;
    ensure_valid_category(db, &body.category, &body.subcategory).await?;

    let sku = super::sku::generate_sku(db, &body.category, &body.subcategory).await?;

    // Defensive fallback only — `generate_sku`'s atomic per-prefix counter
    // already guarantees uniqueness, so this should never actually fire.
    if repository::product::find_product_by_sku(db, &sku)
        .await?
        .is_some()
    {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::SKU_ALREADY_EXISTS,
            format!("A product with SKU '{sku}' already exists"),
        ));
    }

    let document = ProductDocument {
        id: None,
        key: generate_id(prefixes::PRODUCT),
        sku,
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

    let inserted = repository::product::insert_product(db, document).await?;
    Ok(inserted.into_product())
}

/// Partial update — every field in `body` is optional, so each one falls
/// back to the existing document's value before the merged result is
/// re-validated (numbers, category) as if it were a fresh `create`.
pub(crate) async fn update_product(
    db: &Database,
    id: ObjectId,
    body: UpdateProductRequest,
) -> AppResult<Product> {
    let existing = repository::product::find_product_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

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
    ensure_valid_category(db, &category, &subcategory).await?;

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

    let updated = repository::product::update_product(db, id, set_doc)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    Ok(updated.into_product())
}

/// Deletes and returns the deleted document (so the handler can echo back
/// what was removed), 404ing if it never existed.
pub(crate) async fn delete_product(db: &Database, id: ObjectId) -> AppResult<Product> {
    let deleted = repository::product::delete_product(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    Ok(deleted.into_product())
}

/// Batch delete. Ids that aren't valid `ObjectId`s are silently dropped
/// rather than failing the whole request — a client sending a mixed batch
/// (some stale/malformed ids alongside valid ones) still gets the valid
/// ones deleted instead of an all-or-nothing rejection.
pub(crate) async fn delete_products(db: &Database, product_ids: Vec<String>) -> AppResult<u64> {
    let object_ids: Vec<ObjectId> = product_ids
        .iter()
        .filter_map(|id| ObjectId::parse_str(id).ok())
        .collect();

    repository::product::delete_products(db, object_ids).await
}
