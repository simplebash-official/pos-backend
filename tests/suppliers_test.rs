mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use mongodb::bson::DateTime as BsonDateTime;
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::{config::Config, constants::roles, id::generate_id},
    domain::users::Role,
    modules::inventory::model::{CategoryDocument, SubcategoryDocument},
    modules::suppliers::model::SupplierDocument,
};
use tower::ServiceExt;
use uuid::Uuid;

async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    execute(router, request).await
}

/// Same as `send`, but with an `Authorization: Bearer <token>` header — for
/// `suppliers`/`supplier_products`/`purchases`, where every route (reads
/// included) requires `AdminUser`.
async fn send_authed(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    execute(router, request).await
}

/// Mints an Admin-role JWT — every `suppliers`/`supplier_products`/
/// `purchases` route requires `AdminUser`, which only checks `role`, never
/// `permissions`, so an empty permission list is correct here.
fn admin_token(config: &Config) -> String {
    common::mint_token(config, Some(Role::Admin), &[])
}

/// Admin token carrying its *real* permission set, unlike `admin_token`'s
/// empty one — needed only to seed product fixtures through the inventory
/// API, whose routes gate on `inventory:write` rather than on the role.
fn inventory_write_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

/// Seeds a category + subcategory directly (bypassing the admin-gated
/// category API), returning `(category_key, subcategory_key)` — the shape
/// `create_product` needs to build a valid `CreateProductRequest`.
async fn seed_category_with_subcategory(
    db: &mongodb::Database,
    subcategory_name: &str,
) -> (String, String) {
    let category_key = generate_id("cat");
    let now = BsonDateTime::now();
    db.collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: category_key.clone(),
            name: format!("Test Category {}", Uuid::new_v4()),
            icon: "Box".to_string(),
            color: "gray".to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .expect("failed to seed category");

    let subcategory_key = generate_id("subcat");
    db.collection::<SubcategoryDocument>("subcategories")
        .insert_one(SubcategoryDocument {
            id: None,
            key: subcategory_key.clone(),
            category_key: category_key.clone(),
            name: subcategory_name.to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .expect("failed to seed subcategory");

    (category_key, subcategory_key)
}

/// Creates a product via the real inventory API and returns its `(id, key)`
/// — the fixture every supplier-product-link and purchase test needs a real
/// product to reference. Authenticates as Admin carrying its real permission
/// set, since `POST /api/inventory/products` requires `inventory:write`.
async fn create_product(app: &common::TestApp, stock_quantity: i64) -> (String, String) {
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(&app.db, "Test Subcategory").await;
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": format!("Test Product {}", Uuid::new_v4()),
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": stock_quantity,
            "minStockThreshold": 1,
        })),
        &inventory_write_token(&app.config),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "failed to create test product: {body}"
    );
    (
        body["data"]["id"].as_str().unwrap().to_string(),
        body["data"]["key"].as_str().unwrap().to_string(),
    )
}

fn sample_supplier_payload() -> Value {
    json!({
        "name": format!("Test Supplier {}", Uuid::new_v4()),
        "contactPerson": "Test Contact",
        "primaryPhone": "077 123 4567",
        "address": "123 Test Street",
        "suppliedCategories": ["Phone Parts"],
        "email": "supplier@example.com",
    })
}

/// Creates a supplier via the real API, returning `(id, key, response_data)`.
async fn create_supplier(router: &axum::Router, token: &str) -> (String, String, Value) {
    let (status, body) = send_authed(
        router,
        "POST",
        "/api/suppliers",
        Some(sample_supplier_payload()),
        token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "failed to create test supplier: {body}"
    );
    let id = body["data"]["id"].as_str().unwrap().to_string();
    let key = body["data"]["key"].as_str().unwrap().to_string();
    (id, key, body["data"].clone())
}

// ============================================================================
// Supplier CRUD
// ============================================================================

#[tokio::test]
async fn create_supplier_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(sample_supplier_payload()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn suppliers_endpoints_require_admin_role() {
    let app = common::spawn_app().await;

    let (status, body) = send(&app.router, "GET", "/api/suppliers", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let non_admin = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = send_authed(&app.router, "GET", "/api/suppliers", None, &non_admin).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ADMIN_REQUIRED");
}

#[tokio::test]
async fn create_supplier_validates_required_fields() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": "A",
            "contactPerson": "",
            "primaryPhone": "",
            "address": "",
            "suppliedCategories": [],
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn create_supplier_validates_email_format() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let mut payload = sample_supplier_payload();
    payload["email"] = json!("not-an-email");
    let (status, body) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn create_and_get_supplier() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (id, _key, data) = create_supplier(&app.router, &token).await;
    assert_eq!(data["primaryPhone"], "077 123 4567");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["id"], id);
}

#[tokio::test]
async fn list_suppliers_filters_by_search_and_category() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let unique = Uuid::new_v4().simple().to_string();
    let mut payload = sample_supplier_payload();
    payload["name"] = json!(format!("FindableSupplier{unique}"));
    payload["suppliedCategories"] = json!([format!("UniqueTag{unique}")]);
    send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers?search=FindableSupplier{unique}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let suppliers = body["data"]["suppliers"].as_array().unwrap();
    assert_eq!(suppliers.len(), 1, "{body}");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers?category=UniqueTag{unique}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let suppliers = body["data"]["suppliers"].as_array().unwrap();
    assert_eq!(suppliers.len(), 1, "{body}");
}

#[tokio::test]
async fn put_replaces_supplier_clearing_omitted_optional_fields() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _key, _) = create_supplier(&app.router, &token).await;

    let replace_payload = json!({
        "name": "Replaced Supplier",
        "contactPerson": "New Contact",
        "primaryPhone": "011 999 8888",
        "address": "456 New Street",
        "suppliedCategories": ["New Category"],
    });
    let (status, body) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/suppliers/{id}"),
        Some(replace_payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["name"], "Replaced Supplier");
    assert!(
        body["data"].get("email").is_none(),
        "email should be cleared by PUT: {body}"
    );
}

#[tokio::test]
async fn patch_partial_updates_supplier_leaving_other_fields_unchanged() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _key, original) = create_supplier(&app.router, &token).await;

    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/suppliers/{id}"),
        Some(json!({ "notes": "Updated notes" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["notes"], "Updated notes");
    assert_eq!(body["data"]["primaryPhone"], original["primaryPhone"]);
    assert_eq!(body["data"]["email"], original["email"]);
}

#[tokio::test]
async fn delete_supplier_requires_auth_and_removes_it() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _key, _) = create_supplier(&app.router, &token).await;

    let (status, _) = send(&app.router, "DELETE", &format!("/api/suppliers/{id}"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/suppliers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn batch_delete_suppliers_skips_invalid_ids() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id_one, _, _) = create_supplier(&app.router, &token).await;
    let (id_two, _, _) = create_supplier(&app.router, &token).await;

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        "/api/suppliers/batch",
        Some(json!({ "ids": [id_one, id_two, "not-an-object-id"] })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["deletedCount"], 2);
}

#[tokio::test]
async fn supplier_categories_endpoint_returns_distinct_tags() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let unique = Uuid::new_v4().simple().to_string();
    let mut payload = sample_supplier_payload();
    payload["suppliedCategories"] = json!([format!("Tag{unique}")]);
    send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/suppliers/categories",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let categories = body["data"]["categories"].as_array().unwrap();
    assert!(
        categories
            .iter()
            .any(|c| c == &json!(format!("Tag{unique}"))),
        "{body}"
    );
}

// ============================================================================
// Supplier-product links
// ============================================================================

#[tokio::test]
async fn upsert_link_creates_then_updates_in_place() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "costPriceCents": 5000,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["costPriceCents"], 5000);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "costPriceCents": 6000,
            "notes": "Updated",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["costPriceCents"], 6000);
    assert_eq!(body["data"]["notes"], "Updated");
    // Updating in place must bump the sync `version` rather than re-create the
    // link, and must not leave a `deleted_at` tombstone behind (a present-but-
    // null one would hide the link from every `$exists: false` read below).
    assert_eq!(body["data"]["version"], 2, "{body}");
    assert!(body["data"]["deletedAt"].is_null(), "{body}");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/supplier-products?supplierKey={supplier_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let links = body["data"]["links"].as_array().unwrap();
    assert_eq!(
        links.len(),
        1,
        "upsert must not create a duplicate link: {body}"
    );
}

#[tokio::test]
async fn supplier_products_endpoints_require_admin_role() {
    let app = common::spawn_app().await;

    let (status, body) = send(&app.router, "GET", "/api/supplier-products", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let non_admin = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/supplier-products",
        None,
        &non_admin,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ADMIN_REQUIRED");
}

/// `supplierKey`/`productKey` are optional: an unfiltered `GET` returns the
/// whole collection paginated, which is what a sync client pulls on a first
/// full download. (Assertions stay on the pagination envelope rather than on
/// specific rows — the test database is shared and never cleaned, so page 1 of
/// a `created_at`-ascending listing holds whatever the oldest links happen to
/// be.)
#[tokio::test]
async fn list_links_without_keys_returns_paginated_collection() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({ "supplierKey": supplier_key, "productKey": product_key })),
        &token,
    )
    .await;

    let (status, body) =
        send_authed(&app.router, "GET", "/api/supplier-products", None, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let links = body["data"]["links"].as_array().unwrap();
    assert!(links.len() <= 20, "default page size is 20: {body}");
    assert!(
        links
            .iter()
            .all(|link| link["supplierKey"].is_string() && link["productKey"].is_string()),
        "{body}"
    );
    assert_eq!(body["data"]["pagination"]["page"], 1, "{body}");
    assert_eq!(body["data"]["pagination"]["limit"], 20, "{body}");
    assert!(
        body["data"]["pagination"]["total"].as_u64().unwrap() >= 1,
        "the link created above must be counted: {body}"
    );
}

#[tokio::test]
async fn unlink_removes_the_link() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({ "supplierKey": supplier_key, "productKey": product_key })),
        &token,
    )
    .await;

    let unlink_uri = format!("/api/supplier-products/{supplier_key}/{product_key}");
    let (status, _) = send_authed(&app.router, "DELETE", &unlink_uri, None, &token).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_authed(&app.router, "DELETE", &unlink_uri, None, &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bulk_replace_preserves_existing_link_metadata() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_one_id, product_one_key) = create_product(&app, 10).await;
    let (_product_two_id, product_two_key) = create_product(&app, 10).await;
    let (_product_three_id, product_three_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_one_key,
            "costPriceCents": 4000,
            "notes": "Keep me",
        })),
        &token,
    )
    .await;

    let (status, body) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/supplier-products/bulk/{supplier_key}"),
        Some(json!({ "productKeys": [product_one_key, product_two_key] })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let links = body["data"].as_array().unwrap();
    assert_eq!(links.len(), 2);
    let kept = links
        .iter()
        .find(|link| link["productKey"] == json!(product_one_key))
        .expect("retained link missing");
    assert_eq!(kept["costPriceCents"], 4000);
    assert_eq!(kept["notes"], "Keep me");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/supplier-products?supplierKey={supplier_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let links = body["data"]["links"].as_array().unwrap();
    assert!(
        !links
            .iter()
            .any(|link| link["productKey"] == json!(product_three_key)),
        "unlisted product must not be linked: {body}"
    );
}

// ============================================================================
// Purchases
// ============================================================================

#[tokio::test]
async fn record_purchase_increments_stock_and_writes_movement() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (product_id, product_key) = create_product(&app, 10).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/purchases",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "quantity": 25,
            "unitCostCents": 1500,
            "date": "2026-01-15T10:00:00Z",
            "referenceNo": "INV-TEST-1",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["totalCostCents"], 25 * 1500);
    assert_eq!(body["data"]["supplier"]["key"], supplier_key);
    assert_eq!(body["data"]["product"]["key"], product_key);
    let purchase_key = body["data"]["key"].as_str().unwrap().to_string();

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
        &inventory_write_token(&app.config),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["stockQuantity"], 35);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{product_id}/movements"),
        None,
        &inventory_write_token(&app.config),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let movements = body["data"]["movements"].as_array().unwrap();
    let movement = movements
        .iter()
        .find(|m| m["referenceId"] == json!(purchase_key))
        .expect("purchase movement not found");
    assert_eq!(movement["type"], "purchase_receipt");
    assert_eq!(movement["quantityDelta"], 25);
}

#[tokio::test]
async fn purchases_endpoints_require_admin_role() {
    let app = common::spawn_app().await;

    let (status, body) = send(&app.router, "GET", "/api/purchases", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let non_admin = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = send_authed(&app.router, "GET", "/api/purchases", None, &non_admin).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ADMIN_REQUIRED");
}

#[tokio::test]
async fn record_purchase_requires_auth_and_validates_quantity() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    let payload = json!({
        "supplierKey": supplier_key,
        "productKey": product_key,
        "quantity": 5,
        "unitCostCents": 1000,
        "date": "2026-01-15T10:00:00Z",
    });

    let (status, _) = send(&app.router, "POST", "/api/purchases", Some(payload.clone())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let mut invalid = payload;
    invalid["quantity"] = json!(0);
    let (status, _) =
        send_authed(&app.router, "POST", "/api/purchases", Some(invalid), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Same optional-filter contract as
/// `list_links_without_keys_returns_paginated_collection`: an unfiltered
/// `GET /purchases` is a full-collection sync pull, not a 400.
#[tokio::test]
async fn list_purchases_without_keys_returns_paginated_collection() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/purchases",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "quantity": 3,
            "unitCostCents": 1500,
            "date": "2026-01-15T10:00:00Z",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = send_authed(&app.router, "GET", "/api/purchases", None, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let purchases = body["data"]["purchases"].as_array().unwrap();
    assert!(purchases.len() <= 20, "default page size is 20: {body}");
    assert!(
        purchases
            .iter()
            .all(|purchase| purchase["supplierKey"].is_string()),
        "{body}"
    );
    assert_eq!(body["data"]["pagination"]["page"], 1, "{body}");
    assert_eq!(body["data"]["pagination"]["limit"], 20, "{body}");
    assert!(
        body["data"]["pagination"]["total"].as_u64().unwrap() >= 1,
        "the purchase recorded above must be counted: {body}"
    );
}

// ============================================================================
// Cross-module guard
// ============================================================================

#[tokio::test]
async fn deleting_a_supplier_with_purchase_history_is_blocked() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/purchases",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "quantity": 5,
            "unitCostCents": 1000,
            "date": "2026-01-15T10:00:00Z",
        })),
        &token,
    )
    .await;

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/suppliers/{supplier_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "SUPPLIER_HAS_PURCHASES");
}

#[tokio::test]
async fn suppliers_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send(&app.router, "GET", "/api/suppliers/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Inserts a minimal `SupplierDocument` directly, so the stats test can
/// control `contact_person`/`supplied_categories`/`deleted_at` precisely.
async fn seed_supplier_with(
    db: &mongodb::Database,
    contact_person: &str,
    supplied_categories: Vec<String>,
    deleted: bool,
) {
    let key = generate_id("sup");
    let now = BsonDateTime::now();
    db.collection::<SupplierDocument>("suppliers")
        .insert_one(SupplierDocument {
            id: None,
            key,
            name: format!("Stats Test Supplier {}", Uuid::new_v4()),
            contact_person: contact_person.to_string(),
            primary_phone: "0770000000".to_string(),
            secondary_phone: None,
            address: "123 Test St".to_string(),
            supplied_categories,
            email: None,
            notes: None,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: if deleted { Some(now) } else { None },
            updated_by_device: None,
        })
        .await
        .expect("failed to seed supplier");
}

#[tokio::test]
async fn suppliers_stats_endpoint_returns_totals_categories_and_direct_contacts() {
    // `spawn_app`'s DB is shared across the suite, and `/suppliers/stats`
    // aggregates the WHOLE collection with no filter — connect an isolated
    // throwaway database instead, same as the billing/repairs/print-jobs
    // stats tests.
    let app = common::spawn_app().await;
    let db_name = format!("jtstats_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let db = simplebash_pos_backend::clients::mongo::connect(&app.config.mongodb_uri, &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    seed_supplier_with(
        &db,
        "Kasun Silva",
        vec!["Phone Parts".to_string(), "Cables".to_string()],
        false,
    )
    .await;
    seed_supplier_with(&db, "", vec!["Phone Parts".to_string()], false).await;
    seed_supplier_with(&db, "Nadeesha Perera", vec!["Screens".to_string()], false).await;
    // Soft-deleted — must be excluded from both counts.
    seed_supplier_with(&db, "Deleted Contact", vec!["Batteries".to_string()], true).await;

    let db_handle = simplebash_pos_backend::clients::db::Db::from_mongo(db.clone());
    let stats = simplebash_pos_backend::modules::suppliers::service::get_supplier_stats(&db_handle)
        .await
        .expect("get_supplier_stats should succeed");

    assert_eq!(
        stats.total_suppliers, 3,
        "the soft-deleted supplier must be excluded"
    );
    assert_eq!(stats.direct_contacts_count, 2);
    // `supply_categories_count` reuses `distinct_supplied_categories`, the
    // same repository fn `GET /suppliers/categories` already uses — like
    // that endpoint, it's unfiltered by `deleted_at`, so the deleted
    // supplier's "Batteries" tag still counts here.
    assert_eq!(
        stats.supply_categories_count, 4,
        "Phone Parts, Cables, Screens, Batteries"
    );

    db.drop().await.ok();
}
