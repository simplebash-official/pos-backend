// Business rules for product CRUD/listing: price/stock invariants, SKU
// uniqueness, and category/subcategory validation against the
// `categories`/`subcategories` collections. Delegates all Mongo access to
// `repository::product`/`repository::category`/`repository::subcategory`.

use axum::http::StatusCode;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        utils::{build_bson_regex, calculate_pagination},
    },
    domain::inventory::{
        BarcodeSource, CreateProductRequest, PaginationMeta, Product, ProductListQuery,
        ProductListResponse, UpdateProductRequest,
    },
    modules::inventory::{model::ProductDocument, repository},
};

/// Maps a client-supplied `sortBy` value to the corresponding document field.
/// Whitelisted rather than passed straight through, since sort keys end up
/// directly in a Mongo `sort` document.
pub(crate) fn sort_field_for(sort_by: Option<&str>) -> &'static str {
    match sort_by {
        Some("name") => "name",
        Some("sku") => "sku",
        Some("category") => "category_key",
        Some("subcategory") => "subcategory_key",
        Some("costPriceCents") => "cost_price_cents",
        Some("sellingPriceCents") => "selling_price_cents",
        Some("stockQuantity") => "stock_quantity",
        Some("minStockThreshold") => "min_stock_threshold",
        Some("createdAt") => "created_at",
        Some("updatedAt") => "updated_at",
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

/// Checks `subcategory_key` names a subcategory that both exists and
/// belongs to `category_key` — the DB-backed single source of truth for
/// both the category API and this validation. Returns the resolved
/// category/subcategory names (needed for SKU generation) so callers don't
/// have to look them up a second time. Like the rest of this codebase,
/// there's no transaction wrapping this check and the subsequent product
/// write — a category/subcategory deleted in between is an accepted,
/// pre-existing class of risk here, not one this validation newly guards
/// against.
async fn ensure_valid_category(
    db: &Db,
    category_key: &str,
    subcategory_key: &str,
) -> AppResult<(String, String)> {
    let category = repository::category::find_category_by_key(db, category_key)
        .await?
        .ok_or_else(|| AppError::validation(format!("'{category_key}' is not a valid category")))?;

    let subcategory = repository::subcategory::find_subcategory_by_key(db, subcategory_key)
        .await?
        .filter(|document| document.category_key == category_key)
        .ok_or_else(|| {
            AppError::validation(format!(
                "'{subcategory_key}' is not a valid subcategory of '{category_key}'"
            ))
        })?;

    Ok((category.name, subcategory.name))
}

/// Resolves display names for a single product's `category_key`/
/// `subcategory_key` for a read-only response — unlike `ensure_valid_category`,
/// this never fails; a key that no longer resolves (e.g. stale data) just
/// falls back to displaying the key itself rather than 500ing a read.
async fn resolve_display_names(
    db: &Db,
    category_key: &str,
    subcategory_key: &str,
) -> AppResult<(String, String)> {
    let category_name = repository::category::find_category_by_key(db, category_key)
        .await?
        .map(|document| document.name)
        .unwrap_or_else(|| category_key.to_string());
    let subcategory_name = repository::subcategory::find_subcategory_by_key(db, subcategory_key)
        .await?
        .map(|document| document.name)
        .unwrap_or_else(|| subcategory_key.to_string());

    Ok((category_name, subcategory_name))
}

/// Cross-module batch lookup — `purchases::service::purchase::list_purchases`
/// calls this (never `inventory::repository` directly, which is private to
/// this module) to enrich a page of purchase history with product display
/// data in one query instead of one `get_product_by_key` call per row.
pub(crate) async fn get_products_by_keys(db: &Db, keys: &[String]) -> AppResult<Vec<Product>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let filter = doc! { "key": { "$in": keys } };
    let (items, _total) = repository::product::list_products_with_display_names(
        db,
        filter,
        doc! {},
        0,
        keys.len() as i64,
    )
    .await?;
    Ok(items)
}

/// Builds the Mongo filter/sort from query params (category, subcategory,
/// free-text search across name/sku/barcode/category/subcategory, and the low-stock flag) and
/// delegates execution + pagination math to `repository::product::list_products_with_display_names`.
/// Joining categories and subcategories happens inside MongoDB via `$lookup` in a single query.
pub async fn list_products(db: &Db, query: ProductListQuery) -> AppResult<ProductListResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(category_key) = query.category_key.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "category_key": category_key });
    }
    if let Some(subcategory_key) = query.subcategory_key.filter(|s| !s.is_empty()) {
        and_clauses.push(doc! { "subcategory_key": subcategory_key });
    }
    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = build_bson_regex(&search);
        let matched_cat_keys =
            repository::category::find_category_keys_by_name_pattern(db, &pattern).await?;
        let matched_subcat_keys =
            repository::subcategory::find_subcategory_keys_by_name_pattern(db, &pattern).await?;

        let mut or_clauses = vec![
            doc! { "name": { "$regex": pattern.clone() } },
            doc! { "sku": { "$regex": pattern.clone() } },
            doc! { "barcode": { "$regex": pattern } },
        ];
        if !matched_cat_keys.is_empty() {
            or_clauses.push(doc! { "category_key": { "$in": matched_cat_keys } });
        }
        if !matched_subcat_keys.is_empty() {
            or_clauses.push(doc! { "subcategory_key": { "$in": matched_subcat_keys } });
        }
        and_clauses.push(doc! { "$or": or_clauses });
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

    let (items, total) = repository::product::list_products_with_display_names(
        db,
        filter,
        doc! { sort_field: sort_order },
        skip,
        limit as i64,
    )
    .await?;

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
pub(crate) async fn get_product(db: &Db, id: ObjectId) -> AppResult<Product> {
    let document = repository::product::find_product_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    let (category_name, subcategory_name) =
        resolve_display_names(db, &document.category_key, &document.subcategory_key).await?;

    Ok(document.into_product(category_name, subcategory_name))
}

/// Same as `get_product`, looked up by `key` instead of `ObjectId` — the
/// cross-module entry point `supplier_products`/`purchases` call to validate
/// a `productKey` and to enrich their own responses with product display
/// data, since those modules can't reach `inventory::repository` directly
/// (only `inventory::service` is `pub`).
pub(crate) async fn get_product_by_key(db: &Db, key: &str) -> AppResult<Product> {
    let document = repository::product::find_product_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    let (category_name, subcategory_name) =
        resolve_display_names(db, &document.category_key, &document.subcategory_key).await?;

    Ok(document.into_product(category_name, subcategory_name))
}

/// Exact barcode lookup for a scan — resolves the one product carrying
/// `barcode`, 404ing with `PRODUCT_NOT_FOUND` when none does. Unlike the
/// `search` list param (a substring regex that can return several rows) this
/// is an anchored equality match, so a scanner gets an unambiguous result.
pub(crate) async fn get_product_by_barcode(db: &Db, barcode: &str) -> AppResult<Product> {
    let document = repository::product::find_product_by_barcode(db, barcode)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    let (category_name, subcategory_name) =
        resolve_display_names(db, &document.category_key, &document.subcategory_key).await?;

    Ok(document.into_product(category_name, subcategory_name))
}

/// Loose validation for a staff-entered manual barcode: numeric digits
/// only, 8-14 characters — spans common real-world formats (EAN-8/UPC-A/
/// EAN-13/GTIN-14) a scanned product might already carry. Deliberately does
/// not require the EAN-13 checksum to be valid — that check only applies to
/// codes this system generates itself (see `barcode::service::ean13`).
fn validate_manual_barcode(value: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || !trimmed.chars().all(|c| c.is_ascii_digit())
        || !(8..=14).contains(&trimmed.len())
    {
        return Err(AppError::validation("Barcode must be 8-14 numeric digits"));
    }
    Ok(())
}

/// 409s with `BARCODE_ALREADY_EXISTS` if another product already carries
/// `barcode`. Used both for a real, expected-to-sometimes-fire manual-entry
/// collision and as a defensive check after generation (which should never
/// actually fire, mirroring the SKU defensive check below).
async fn ensure_barcode_available(db: &Db, barcode: &str) -> AppResult<()> {
    if repository::product::find_product_by_barcode(db, barcode)
        .await?
        .is_some()
    {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::BARCODE_ALREADY_EXISTS,
            format!("A product with barcode '{barcode}' already exists"),
        ));
    }
    Ok(())
}

/// Resolves what (if anything) `create_product` should persist for
/// `barcode`/`barcode_source`, enforcing that a manual value and
/// auto-generation are mutually exclusive. A product supplying neither is fine,
/// since a barcode can never be added later (it's immutable after creation).
async fn resolve_barcode(
    db: &Db,
    barcode: Option<String>,
    auto_generate_barcode: bool,
) -> AppResult<(Option<String>, Option<BarcodeSource>)> {
    match (barcode, auto_generate_barcode) {
        (Some(_), true) => Err(AppError::validation(
            "Provide either 'barcode' or 'autoGenerateBarcode', not both",
        )),
        (Some(manual), false) => {
            validate_manual_barcode(&manual)?;
            ensure_barcode_available(db, &manual).await?;
            Ok((Some(manual), Some(BarcodeSource::Manual)))
        }
        (None, true) => {
            let generated = crate::modules::barcode::service::generator::generate(
                db,
                crate::modules::barcode::service::generator::namespaces::PRODUCT,
            )
            .await?;
            // Defensive fallback only — the atomic namespaced counter plus
            // fixed reserved prefix make a collision structurally
            // impossible, same "should never actually fire" reasoning as
            // the SKU defensive check below.
            ensure_barcode_available(db, &generated).await?;
            Ok((Some(generated), Some(BarcodeSource::Generated)))
        }
        (None, false) => Ok((None, None)),
    }
}

/// Validates required fields, price/stock invariants, category
/// membership, and any optional supplier intake list, then generates the product's
/// SKU from its category/subcategory names (see `service::sku::generate_sku`) —
/// validation runs first so an invalid category/supplier never consumes a sequence number.
pub async fn create_product(db: &Db, body: CreateProductRequest) -> AppResult<Product> {
    if body.name.trim().is_empty() {
        return Err(AppError::validation("Product name is required"));
    }
    validate_product_numbers(
        body.selling_price_cents,
        body.cost_price_cents,
        body.stock_quantity,
        body.min_stock_threshold,
    )?;
    let (category_name, subcategory_name) =
        ensure_valid_category(db, &body.category_key, &body.subcategory_key).await?;

    // Fail fast: validate suppliers list upfront
    if !body.suppliers.is_empty() {
        let mut seen_keys = std::collections::HashSet::new();
        for intake in &body.suppliers {
            if !seen_keys.insert(&intake.supplier_key) {
                return Err(AppError::validation(format!(
                    "Duplicate supplierKey '{}' in suppliers list",
                    intake.supplier_key
                )));
            }
            if intake.quantity < 1 {
                return Err(AppError::validation(
                    "Supplier intake quantity must be at least 1",
                ));
            }
            if intake.cost_price_cents < 0 {
                return Err(AppError::validation(
                    "Supplier intake cost price cannot be negative",
                ));
            }
            crate::modules::suppliers::service::get_supplier_by_key(db, &intake.supplier_key)
                .await?;
        }
    }

    let (barcode, barcode_source) =
        resolve_barcode(db, body.barcode, body.auto_generate_barcode).await?;

    let sku = super::sku::generate_sku(db, &category_name, &subcategory_name).await?;

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

    let initial_stock = if body.suppliers.is_empty() {
        body.stock_quantity
    } else {
        0
    };

    let default_cost_price = if body.cost_price_cents == 0 && !body.suppliers.is_empty() {
        body.suppliers[0].cost_price_cents
    } else {
        body.cost_price_cents
    };

    let now = BsonDateTime::now();
    let document = ProductDocument {
        id: None,
        key: generate_id(prefixes::PRODUCT),
        sku,
        barcode,
        barcode_source,
        name: body.name,
        category_key: body.category_key,
        subcategory_key: body.subcategory_key,
        cost_price_cents: default_cost_price,
        selling_price_cents: body.selling_price_cents,
        stock_quantity: initial_stock,
        min_stock_threshold: body.min_stock_threshold,
        is_serialized: body.is_serialized,
        warranty_months: body.warranty_months,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        updated_by_device: None,
    };

    let inserted = repository::product::insert_product(db, document).await?;
    let product_object_id = inserted.id.expect("inserted product must have an id");

    // Process each supplier intake
    for intake in body.suppliers {
        crate::modules::supplier_products::service::link::upsert_link(
            db,
            crate::domain::supplier_products::UpsertSupplierProductLinkRequest {
                supplier_key: intake.supplier_key.clone(),
                product_key: inserted.key.clone(),
                cost_price_cents: Some(intake.cost_price_cents),
                notes: intake.notes.clone(),
            },
        )
        .await?;

        crate::modules::purchases::service::purchase::record_purchase(
            db,
            crate::domain::purchases::CreatePurchaseRequest {
                supplier_key: intake.supplier_key,
                product_key: inserted.key.clone(),
                quantity: intake.quantity,
                unit_cost_cents: intake.cost_price_cents,
                date: chrono::Utc::now(),
                reference_no: intake.reference_no,
                notes: intake.notes,
                // A brand-new serialized product's initial supplier intake
                // has no UI/field for supplying serials yet — creating a
                // serialized product with bundled initial stock through
                // this path is out of scope; use a separate purchase-intake
                // call afterward for a serialized product's first stock.
                serial_numbers: None,
            },
        )
        .await?;
    }

    let final_product = repository::product::find_product_by_id(db, product_object_id)
        .await?
        .expect("product was just created and must exist");

    Ok(final_product.into_product(category_name, subcategory_name))
}

/// Partial update — every field in `body` is optional, so each one falls
/// back to the existing document's value before the merged result is
/// re-validated (numbers, category) as if it were a fresh `create`.
pub(crate) async fn update_product(
    db: &Db,
    id: ObjectId,
    body: UpdateProductRequest,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Product> {
    let existing = repository::product::find_product_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    if let Some(expected) = expected_version
        && existing.version != expected
    {
        let mut conflicting = Vec::new();
        if let Some(ref name) = body.name
            && name != &existing.name
        {
            conflicting.push("name");
        }
        if let Some(price) = body.selling_price_cents
            && price != existing.selling_price_cents
        {
            conflicting.push("sellingPriceCents");
        }
        if let Some(cost) = body.cost_price_cents
            && cost != existing.cost_price_cents
        {
            conflicting.push("costPriceCents");
        }
        if let Some(ref cat) = body.category_key
            && cat != &existing.category_key
        {
            conflicting.push("categoryKey");
        }
        if let Some(ref subcat) = body.subcategory_key
            && subcat != &existing.subcategory_key
        {
            conflicting.push("subcategoryKey");
        }
        if let Some(qty) = body.stock_quantity
            && qty != existing.stock_quantity
        {
            conflicting.push("stockQuantity");
        }
        if let Some(min) = body.min_stock_threshold
            && min != existing.min_stock_threshold
        {
            conflicting.push("minStockThreshold");
        }
        if let Some(ref barcode) = body.barcode
            && Some(barcode) != existing.barcode.as_ref()
        {
            conflicting.push("barcode");
        }

        let (category_name, subcategory_name) =
            resolve_display_names(db, &existing.category_key, &existing.subcategory_key).await?;
        let server_doc = existing
            .clone()
            .into_product(category_name, subcategory_name);

        return Err(AppError::conflict_with_details(
            codes::VERSION_CONFLICT,
            "This product was changed on another device.",
            serde_json::json!({
                "expectedVersion": expected,
                "serverVersion": existing.version,
                "updatedByDevice": existing.updated_by_device,
                "server": server_doc,
                "conflictingFields": conflicting,
            }),
        ));
    }

    if let Some(name) = &body.name
        && name.trim().is_empty()
    {
        return Err(AppError::validation("Product name cannot be empty"));
    }

    // Resolve a barcode edit before `existing` is partially moved below. A
    // value equal to the current one is a no-op (no needless write, no
    // `barcode_source` flip); a new value is validated and checked for a
    // collision against every *other* product.
    let barcode_change: Option<String> = match body.barcode {
        Some(ref candidate) if existing.barcode.as_deref() == Some(candidate.as_str()) => None,
        Some(candidate) => {
            validate_manual_barcode(&candidate)?;
            if repository::product::find_product_by_barcode_excluding(db, &candidate, id)
                .await?
                .is_some()
            {
                return Err(AppError::custom(
                    StatusCode::CONFLICT,
                    codes::BARCODE_ALREADY_EXISTS,
                    format!("A product with barcode '{candidate}' already exists"),
                ));
            }
            Some(candidate)
        }
        None => None,
    };

    let category_key = body.category_key.unwrap_or(existing.category_key);
    let subcategory_key = body.subcategory_key.unwrap_or(existing.subcategory_key);
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
    let (category_name, subcategory_name) =
        ensure_valid_category(db, &category_key, &subcategory_key).await?;

    let mut set_doc = doc! {
        "category_key": &category_key,
        "subcategory_key": &subcategory_key,
        "selling_price_cents": selling_price_cents,
        "cost_price_cents": cost_price_cents,
        "stock_quantity": stock_quantity,
        "min_stock_threshold": min_stock_threshold,
        "updated_at": BsonDateTime::now(),
    };
    if let Some(name) = body.name {
        set_doc.insert("name", name);
    }
    if let Some(is_serialized) = body.is_serialized {
        set_doc.insert("is_serialized", is_serialized);
    }
    if let Some(warranty_months) = body.warranty_months {
        set_doc.insert("warranty_months", warranty_months);
    }
    if let Some(barcode) = barcode_change {
        set_doc.insert("barcode", barcode);
        // A staff-entered barcode is always "manual", even if the product
        // previously carried a system-generated one. Serialize through serde
        // so it stays in sync with `BarcodeSource`'s wire form.
        let source = mongodb::bson::serialize_to_bson(&BarcodeSource::Manual)
            .expect("BarcodeSource serializes to a plain string");
        set_doc.insert("barcode_source", source);
    }
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }

    let updated = repository::product::update_product(db, id, set_doc)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    Ok(updated.into_product(category_name, subcategory_name))
}

/// Deletes and returns the deleted document (so the handler can echo back
/// what was removed), 404ing if it never existed. Also cascades the delete
/// onto any `supplier_products` links pointing at this product's `key` —
/// mirrors the category/subcategory cascade in `service::category`, just
/// across a module boundary rather than within one.
pub(crate) async fn delete_product(
    db: &Db,
    id: ObjectId,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Product> {
    let existing = repository::product::find_product_by_id(db, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    if let Some(expected) = expected_version
        && existing.version != expected
    {
        let (category_name, subcategory_name) =
            resolve_display_names(db, &existing.category_key, &existing.subcategory_key).await?;
        let server_doc = existing
            .clone()
            .into_product(category_name, subcategory_name);

        return Err(AppError::conflict_with_details(
            codes::VERSION_CONFLICT,
            "This product was changed on another device.",
            serde_json::json!({
                "expectedVersion": expected,
                "serverVersion": existing.version,
                "updatedByDevice": existing.updated_by_device,
                "server": server_doc,
                "conflictingFields": Vec::<String>::new(),
            }),
        ));
    }

    let deleted = repository::product::delete_product(db, id, device_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
        })?;

    crate::modules::supplier_products::service::link::delete_links_for_product(db, &deleted.key)
        .await?;

    let (category_name, subcategory_name) =
        resolve_display_names(db, &deleted.category_key, &deleted.subcategory_key).await?;

    Ok(deleted.into_product(category_name, subcategory_name))
}

/// Batch delete. Ids that aren't valid `ObjectId`s are silently dropped
/// rather than failing the whole request — a client sending a mixed batch
/// (some stale/malformed ids alongside valid ones) still gets the valid
/// ones deleted instead of an all-or-nothing rejection. Cascades
/// `supplier_products` link cleanup for every product actually deleted,
/// same as the single-delete path.
pub(crate) async fn delete_products(db: &Db, product_ids: Vec<String>) -> AppResult<u64> {
    let object_ids: Vec<ObjectId> = product_ids
        .iter()
        .filter_map(|id| ObjectId::parse_str(id).ok())
        .collect();

    // Keys are needed for the `supplier_products` cascade below, so they
    // must be read before the delete removes the documents they came from.
    let keys: Vec<String> = repository::product::find_products_by_ids(db, &object_ids)
        .await?
        .into_iter()
        .map(|document| document.key)
        .collect();

    let deleted_count = repository::product::delete_products(db, object_ids).await?;

    for key in keys {
        crate::modules::supplier_products::service::link::delete_links_for_product(db, &key)
            .await?;
    }

    Ok(deleted_count)
}

/// Converts a page of raw `products` documents — as read by the sync
/// module's cursor scan — into the exact `Product` shape `GET /products`
/// returns. The sync delta feed and the REST snapshot feed have to be
/// byte-identical: the client mirrors both into the same local table, so a
/// row that arrives by delta must not be missing the `category`/
/// `subcategory` display names the read path resolves.
///
/// Names are resolved with two collection reads for the whole page (the
/// category/subcategory collections are small reference data) rather than
/// the per-row `resolve_display_names` used by single-document reads.
pub(crate) async fn hydrate_sync_documents(
    db: &Db,
    documents: Vec<Document>,
) -> AppResult<Vec<Product>> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }

    let categories = repository::category::list_categories_with_subcategories(db).await?;

    let mut category_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut subcategory_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for (category, subcategories) in categories {
        category_names.insert(category.key.clone(), category.name);
        for subcategory in subcategories {
            subcategory_names.insert(subcategory.key, subcategory.name);
        }
    }

    let mut products = Vec::with_capacity(documents.len());
    for document in documents {
        let product = bson::deserialize_from_document::<ProductDocument>(document)?;

        // A key that no longer resolves falls back to displaying the key
        // itself — same rule as `resolve_display_names`, so a stale
        // reference degrades identically on both feeds.
        let category_name = category_names
            .get(&product.category_key)
            .cloned()
            .unwrap_or_else(|| product.category_key.clone());
        let subcategory_name = subcategory_names
            .get(&product.subcategory_key)
            .cloned()
            .unwrap_or_else(|| product.subcategory_key.clone());

        products.push(product.into_product(category_name, subcategory_name));
    }

    Ok(products)
}
