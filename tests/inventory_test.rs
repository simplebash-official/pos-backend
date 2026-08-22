mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jana2u_pos_backend::{
    core::{
        config::Config,
        constants::{codes, roles},
        id::generate_id,
    },
    domain::users::Role,
    modules::inventory::model::{CategoryDocument, ProductDocument, SubcategoryDocument},
};
use serde_json::{Value, json};
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

/// The default request helper: authenticates as a Manager, which holds both
/// `inventory:read` and `inventory:write` and so can reach every non-admin
/// inventory route. Every route in this module except the module-status stub
/// requires a token, so a test that isn't *about* authorization should use
/// this rather than restating the auth setup. Use `send_anon` to assert a
/// route rejects an unauthenticated caller, and `send_authed` to drive a
/// route as a specific role.
async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    send_authed(&app.router, method, uri, body, &manager_token(&app.config)).await
}

/// Sends with no `Authorization` header at all — for asserting that a route
/// is actually gated, and for the one route that is intentionally public.
async fn send_anon(
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

/// Same as `send_anon`, but with an `Authorization: Bearer <token>` header,
/// so a test can pick the exact role/permission set it wants to exercise.
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

/// Mints a JWT with the given role claim (or none) and no permissions.
/// Only for the `AdminUser`-gated category/subcategory routes, which check
/// `role` and never `permissions` — a token from here cannot pass the
/// `require_permission` gate on the product/stock/category-read routes.
fn token_with_role(config: &Config, role: Option<Role>) -> String {
    common::mint_token(config, role, &[])
}

fn admin_token(config: &Config) -> String {
    token_with_role(config, Some(Role::Admin))
}

/// Mints a token carrying a role's *real* default permission set, so the
/// permission gates on the product/stock routes see what they would see
/// after an actual login.
fn token_for_role(config: &Config, role: Role) -> String {
    common::mint_token(config, Some(role), roles::default_permissions(role))
}

/// Manager: holds `inventory:read` + `inventory:write`, but is not Admin.
fn manager_token(config: &Config) -> String {
    token_for_role(config, Role::Manager)
}

/// Staff: holds `inventory:read` only — the role that proves reads and
/// writes are gated separately rather than both collapsing to "logged in".
fn staff_token(config: &Config) -> String {
    token_for_role(config, Role::Staff)
}

use mongodb::bson::DateTime as BsonDateTime;

/// Inserts a category directly into the test database rather than through
/// the admin API, so tests that don't care about category management
/// itself (e.g. product CRUD) don't need an admin token just to set up
/// fixture data. Uses a random name so parallel tests never collide with
/// each other or with real seeded data. Returns the category's key.
async fn seed_category(db: &mongodb::Database) -> String {
    let name = format!("Test Category {}", Uuid::new_v4());
    let key = generate_id("cat");
    let now = BsonDateTime::now();
    db.collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: key.clone(),
            name,
            icon: "Box".to_string(),
            color: "blue".to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .expect("failed to seed category");
    key
}

/// Inserts a subcategory directly into the test database under
/// `category_key`. Returns the subcategory's key.
async fn seed_subcategory(db: &mongodb::Database, category_key: &str, name: &str) -> String {
    let key = generate_id("subcat");
    let now = BsonDateTime::now();
    db.collection::<SubcategoryDocument>("subcategories")
        .insert_one(SubcategoryDocument {
            id: None,
            key: key.clone(),
            category_key: category_key.to_string(),
            name: name.to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .expect("failed to seed subcategory");
    key
}

/// Convenience wrapper: seeds a category plus one subcategory under it,
/// returning `(category_key, subcategory_key)` — the shape most product
/// tests need.
async fn seed_category_with_subcategory(
    db: &mongodb::Database,
    subcategory_name: &str,
) -> (String, String) {
    let category_key = seed_category(db).await;
    let subcategory_key = seed_subcategory(db, &category_key, subcategory_name).await;
    (category_key, subcategory_key)
}

#[tokio::test]
async fn product_lifecycle_create_get_update_stock_and_delete() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Test Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 10,
            "minStockThreshold": 3,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["success"], true);
    let id = created["data"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["categoryKey"], category_key);
    assert_eq!(created["data"]["subcategoryKey"], subcategory_key);
    // `category` is seeded as "Test Category <uuid>" -> derived code "TES";
    // "Widgets" -> "WID" (see service::sku::derive_code).
    let sku = created["data"]["sku"].as_str().unwrap().to_string();
    assert!(sku.starts_with("TES-WID-"), "unexpected sku: {sku}");
    assert_eq!(created["data"]["stockQuantity"], 10);

    let (status, fetched) = send(&app, "GET", &format!("/api/inventory/products/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["data"]["id"], id);

    let (status, updated) = send(
        &app,
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "name": "Updated Widget", "sellingPriceCents": 2500 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["name"], "Updated Widget");
    assert_eq!(updated["data"]["sellingPriceCents"], 2500);
    assert_eq!(updated["data"]["sku"], sku);

    let (status, adjusted) = send(
        &app,
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": -4, "reason": "test sale" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(adjusted["data"]["stockQuantity"], 6);
    assert_eq!(adjusted["data"]["previousStockQuantity"], 10);
    assert_eq!(adjusted["data"]["delta"], -4);

    let (status, insufficient) = send(
        &app,
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": -100 })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(insufficient["code"], "INSUFFICIENT_STOCK");
    assert_eq!(insufficient["details"]["available"], 6);
    assert_eq!(insufficient["details"]["requested"], 100);

    let (status, movements) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{id}/movements"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = movements["data"]["movements"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["quantityDelta"], -4);
    assert_eq!(list[0]["type"], "manual_adjustment");

    let (status, deleted) = send(
        &app,
        "DELETE",
        &format!("/api/inventory/products/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["data"]["id"], id);

    let (status, missing) = send(&app, "GET", &format!("/api/inventory/products/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "PRODUCT_NOT_FOUND");
}

#[tokio::test]
async fn create_product_validates_category_and_pricing_and_auto_generates_sequential_skus() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (status, invalid_category) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Bad Category Widget",
            "categoryKey": category_key,
            "subcategoryKey": "subcat_does_not_exist",
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_category["code"], "VALIDATION_ERROR");

    // A subcategory that exists but belongs to a *different* category must
    // also be rejected — proves the category_key/subcategory_key pairing is
    // actually enforced, not just "does this subcategory key exist anywhere".
    let (other_category_key, other_subcategory_key) =
        seed_category_with_subcategory(&app.db, "Gadgets").await;
    let (status, mismatched_category) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Mismatched Widget",
            "categoryKey": category_key,
            "subcategoryKey": other_subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(mismatched_category["code"], "VALIDATION_ERROR");
    let _ = other_category_key;

    let (status, _bad_price) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Free Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 0,
            "stockQuantity": 1,
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Two products created back-to-back under the same category/subcategory
    // should get SKUs sharing a prefix but with sequential, distinct suffixes
    // — proving the atomic per-prefix counter, not client input, drives them.
    let body = json!({
        "name": "Sequential Widget",
        "categoryKey": category_key,
        "subcategoryKey": subcategory_key,
        "costPriceCents": 1000,
        "sellingPriceCents": 2000,
        "stockQuantity": 1,
        "minStockThreshold": 1,
    });
    let (status, first) = send(&app, "POST", "/api/inventory/products", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    let first_sku = first["data"]["sku"].as_str().unwrap().to_string();

    let (status, second) = send(&app, "POST", "/api/inventory/products", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let second_sku = second["data"]["sku"].as_str().unwrap().to_string();

    assert_ne!(first_sku, second_sku);
    let prefix = first_sku.rsplit_once('-').unwrap().0;
    assert!(second_sku.starts_with(&format!("{prefix}-")));
}

/// A unique 10-digit numeric string per call — used for manual-barcode test
/// fixtures so repeated test runs against the persistent test DB (see
/// `common::spawn_app`, which never drops data between runs) never collide
/// with a barcode a previous run already inserted.
fn unique_manual_barcode() -> String {
    let digits: String = Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(10)
        .collect();
    format!("{digits:0<10}")
}

#[tokio::test]
async fn create_product_auto_generates_sequential_ean13_barcodes() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let body = json!({
        "name": "Auto Barcode Widget",
        "categoryKey": category_key,
        "subcategoryKey": subcategory_key,
        "costPriceCents": 1000,
        "sellingPriceCents": 2000,
        "stockQuantity": 1,
        "minStockThreshold": 1,
        "autoGenerateBarcode": true,
    });

    let (status, first) = send(&app, "POST", "/api/inventory/products", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    let first_barcode = first["data"]["barcode"].as_str().unwrap().to_string();
    assert_eq!(first_barcode.len(), 13);
    assert!(first_barcode.chars().all(|c| c.is_ascii_digit()));
    assert!(first_barcode.starts_with("20"));
    assert_eq!(first["data"]["barcodeSource"], "generated");

    let (status, second) = send(&app, "POST", "/api/inventory/products", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED);
    let second_barcode = second["data"]["barcode"].as_str().unwrap().to_string();
    assert_ne!(first_barcode, second_barcode);

    let first_seq: i64 = first_barcode[2..12].parse().unwrap();
    let second_seq: i64 = second_barcode[2..12].parse().unwrap();
    assert_eq!(second_seq, first_seq + 1);
}

#[tokio::test]
async fn create_product_accepts_manual_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let barcode = unique_manual_barcode();

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Manual Barcode Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["barcode"], barcode);
    assert_eq!(created["data"]["barcodeSource"], "manual");
}

#[tokio::test]
async fn create_product_rejects_invalid_manual_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    for bad_barcode in ["abc12345", "1234", "1234567890123456"] {
        let (status, body) = send(
            &app,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Invalid Barcode Widget",
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "costPriceCents": 1000,
                "sellingPriceCents": 2000,
                "stockQuantity": 1,
                "minStockThreshold": 1,
                "barcode": bad_barcode,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "barcode: {bad_barcode}");
        assert_eq!(body["code"], "VALIDATION_ERROR");
    }
}

#[tokio::test]
async fn create_product_rejects_duplicate_manual_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let barcode = unique_manual_barcode();

    let (status, _first) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "First Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, second) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Second Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
            "barcode": barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(second["code"], "BARCODE_ALREADY_EXISTS");
}

#[tokio::test]
async fn create_product_rejects_barcode_and_auto_generate_together() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Conflicting Barcode Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
            "barcode": "12345678",
            "autoGenerateBarcode": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

async fn seed_supplier(db: &mongodb::Database) -> String {
    let supplier_key = generate_id("sup");
    let now = BsonDateTime::now();
    db.collection::<mongodb::bson::Document>("suppliers")
        .insert_one(mongodb::bson::doc! {
            "key": &supplier_key,
            "name": format!("Test Supplier {}", Uuid::new_v4()),
            "contact_person": "Jane Doe",
            "primary_phone": "1234567890",
            "address": "123 Main St",
            "supplied_categories": vec!["Widgets"],
            "created_at": now,
            "updated_at": now,
        })
        .await
        .expect("failed to seed supplier");
    supplier_key
}

#[tokio::test]
async fn create_product_without_barcode_persists_null() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Barcode-Free Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(created["data"]["barcode"].is_null());
    assert!(created["data"]["barcodeSource"].is_null());
}

#[tokio::test]
async fn create_product_with_multiple_suppliers_links_and_records_intake_atomically() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Screens").await;
    let supplier_a_key = seed_supplier(&app.db).await;
    let supplier_b_key = seed_supplier(&app.db).await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "iPhone 15 Pro Screen (Multi-Supplier)",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "sellingPriceCents": 1500,
            "minStockThreshold": 5,
            "suppliers": [
                {
                    "supplierKey": supplier_a_key,
                    "quantity": 30,
                    "costPriceCents": 450,
                    "referenceNo": "INV-SUP-A-001",
                    "notes": "Batch A OEM"
                },
                {
                    "supplierKey": supplier_b_key,
                    "quantity": 20,
                    "costPriceCents": 480,
                    "referenceNo": "INV-SUP-B-002",
                    "notes": "Batch B Premium"
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["stockQuantity"], 50);
    assert_eq!(created["data"]["costPriceCents"], 450);

    let product_key = created["data"]["key"].as_str().unwrap();
    let product_id = created["data"]["id"].as_str().unwrap();

    // 1. Verify supplier_products links for both suppliers
    let link_a = app
        .db
        .collection::<mongodb::bson::Document>("supplier_products")
        .find_one(mongodb::bson::doc! {
            "supplier_key": &supplier_a_key,
            "product_key": product_key,
        })
        .await
        .expect("query link A")
        .expect("link A must exist");
    assert_eq!(link_a.get_i64("cost_price_cents").unwrap(), 450);

    let link_b = app
        .db
        .collection::<mongodb::bson::Document>("supplier_products")
        .find_one(mongodb::bson::doc! {
            "supplier_key": &supplier_b_key,
            "product_key": product_key,
        })
        .await
        .expect("query link B")
        .expect("link B must exist");
    assert_eq!(link_b.get_i64("cost_price_cents").unwrap(), 480);

    // 2. Verify purchases collection has 2 records
    let purchases_count = app
        .db
        .collection::<mongodb::bson::Document>("purchases")
        .count_documents(mongodb::bson::doc! { "product_key": product_key })
        .await
        .expect("count purchases");
    assert_eq!(purchases_count, 2);

    // 3. Verify stock movements for the product
    let (status, movements) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{product_id}/movements"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = movements["data"]["movements"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let total_delta: i64 = items
        .iter()
        .map(|m| m["quantityDelta"].as_i64().unwrap())
        .sum();
    assert_eq!(total_delta, 50);
}

#[tokio::test]
async fn create_product_rejects_duplicate_suppliers_in_same_request() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let supplier_key = seed_supplier(&app.db).await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Duplicate Supplier Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "sellingPriceCents": 2000,
            "suppliers": [
                {
                    "supplierKey": supplier_key,
                    "quantity": 10,
                    "costPriceCents": 1000
                },
                {
                    "supplierKey": supplier_key,
                    "quantity": 5,
                    "costPriceCents": 1100
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn create_product_rejects_nonexistent_supplier_in_intake_list() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let valid_supplier_key = seed_supplier(&app.db).await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Nonexistent Supplier Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "sellingPriceCents": 2000,
            "suppliers": [
                {
                    "supplierKey": valid_supplier_key,
                    "quantity": 10,
                    "costPriceCents": 1000
                },
                {
                    "supplierKey": "sup_nonexistent_999",
                    "quantity": 5,
                    "costPriceCents": 1100
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "SUPPLIER_NOT_FOUND");
}

#[tokio::test]
async fn create_product_rejects_zero_or_negative_supplier_quantity() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let supplier_key = seed_supplier(&app.db).await;

    let (status, body) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Zero Qty Supplier Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "sellingPriceCents": 2000,
            "suppliers": [
                {
                    "supplierKey": supplier_key,
                    "quantity": 0,
                    "costPriceCents": 1000
                }
            ]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn update_product_cannot_change_barcode() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let original_barcode = unique_manual_barcode();

    let (status, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Immutable Barcode Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
            "barcode": original_barcode,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let (status, updated) = send(
        &app,
        "PUT",
        &format!("/api/inventory/products/{id}"),
        Some(json!({ "name": "Renamed Widget", "barcode": unique_manual_barcode() })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["name"], "Renamed Widget");
    assert_eq!(updated["data"]["barcode"], original_barcode);
}

#[tokio::test]
async fn list_products_filters_by_category_and_search() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Findable Gadget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1200,
            "stockQuantity": 5,
            "minStockThreshold": 1,
        })),
    )
    .await;

    send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Unrelated Item",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1200,
            "stockQuantity": 5,
            "minStockThreshold": 1,
        })),
    )
    .await;

    let (status, listed) = send(
        &app,
        "GET",
        &format!("/api/inventory/products?categoryKey={category_key}&search=Findable"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = listed["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "Findable Gadget");
    assert_eq!(listed["data"]["pagination"]["total"], 1);
}

#[tokio::test]
async fn delete_products_batch_removes_multiple() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let mut ids = Vec::new();
    for _ in 0..2 {
        let (_, created) = send(
            &app,
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Batch Widget",
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "costPriceCents": 500,
                "sellingPriceCents": 1200,
                "stockQuantity": 5,
                "minStockThreshold": 1,
            })),
        )
        .await;
        ids.push(created["data"]["id"].as_str().unwrap().to_string());
    }

    let (status, result) = send(
        &app,
        "DELETE",
        "/api/inventory/products",
        Some(json!({ "productIds": ids })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["data"]["deletedCount"], 2);

    for id in ids {
        let (status, _) = send(&app, "GET", &format!("/api/inventory/products/{id}"), None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn low_stock_lists_products_at_or_below_threshold() {
    let app = common::spawn_app().await;
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;

    let (_, created) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Low Stock Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1200,
            "stockQuantity": 2,
            "minStockThreshold": 5,
        })),
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let (status, low_stock) = send(&app, "GET", "/api/inventory/products/low-stock", None).await;
    assert_eq!(status, StatusCode::OK);
    let items = low_stock["data"]["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["id"] == id));
}

#[tokio::test]
async fn category_endpoints_read_seeded_categories() {
    let app = common::spawn_app().await;
    let category_key = seed_category(&app.db).await;
    seed_subcategory(&app.db, &category_key, "Alpha").await;
    seed_subcategory(&app.db, &category_key, "Beta").await;

    let (status, all) = send(&app, "GET", "/api/inventory/categories", None).await;
    assert_eq!(status, StatusCode::OK);
    let categories = all["data"]["categories"].as_array().unwrap();
    assert!(
        categories
            .iter()
            .any(|entry| entry["key"] == category_key
                && entry["subcategories"][0]["name"] == "Alpha")
    );

    let (status, subs) = send(
        &app,
        "GET",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let subcategory_names: Vec<&str> = subs["data"]["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert_eq!(subcategory_names, vec!["Alpha", "Beta"]);

    let (status, missing) = send(
        &app,
        "GET",
        "/api/inventory/categories/cat_does_not_exist/subcategories",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "CATEGORY_NOT_FOUND");

    let (status, valid) = send(&app, "GET", "/api/inventory/categories/valid", None).await;
    assert_eq!(status, StatusCode::OK);
    let valid_categories = valid["data"]["categories"].as_array().unwrap();
    let entry = valid_categories
        .iter()
        .find(|entry| entry["key"] == category_key)
        .expect("seeded category present in valid categories response");
    let valid_subcategory_names: Vec<&str> = entry["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(valid_subcategory_names, vec!["Alpha", "Beta"]);
}

#[tokio::test]
async fn category_admin_endpoints_require_admin_role() {
    let app = common::spawn_app().await;
    let body = Some(json!({
        "name": format!("Unauthorized Category {}", Uuid::new_v4()),
        "icon": "Box",
        "color": "blue",
    }));

    let (status, no_auth) = send_anon(
        &app.router,
        "POST",
        "/api/inventory/categories",
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(no_auth["code"], "UNAUTHORIZED");

    let non_admin = token_with_role(&app.config, Some(Role::Staff));
    let (status, forbidden) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        body,
        &non_admin,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(forbidden["code"], "ADMIN_REQUIRED");
}

#[tokio::test]
async fn category_admin_crud_lifecycle_and_in_use_guards() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let name = format!("Admin Category {}", Uuid::new_v4());

    let (status, created) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": name,
            "icon": "Box",
            "color": "blue",
            "subcategories": ["Alpha"],
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["name"], name);
    let category_key = created["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(created["data"]["subcategories"][0]["name"], "Alpha");
    let alpha_key = created["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, duplicate) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": name, "icon": "Box", "color": "blue" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate["code"], "CATEGORY_ALREADY_EXISTS");

    let (_, product) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Cascade Widget",
            "categoryKey": category_key,
            "subcategoryKey": alpha_key,
            "costPriceCents": 500,
            "sellingPriceCents": 1000,
            "stockQuantity": 1,
            "minStockThreshold": 1,
        })),
    )
    .await;
    let product_id = product["data"]["id"].as_str().unwrap().to_string();

    // Renaming the category is now a pure display-label change — the
    // product's categoryKey (the actual FK) must stay exactly the same.
    let renamed = format!("{name} Renamed");
    let (status, updated) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "name": renamed })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["name"], renamed);
    assert_eq!(updated["data"]["key"], category_key);

    let (_, fetched_product) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
    )
    .await;
    assert_eq!(fetched_product["data"]["categoryKey"], category_key);
    assert_eq!(fetched_product["data"]["category"], renamed);

    // Category still has a product on it, and the subcategory does too.
    let (status, category_in_use) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(category_in_use["code"], "CATEGORY_IN_USE");

    let (status, subcategory_in_use) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}/subcategories/{alpha_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(subcategory_in_use["code"], "SUBCATEGORY_IN_USE");

    // Add a second subcategory, then free up the product so both deletes
    // (subcategory, then category) can succeed.
    let (status, with_beta) = send_authed(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Beta" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let subcategory_names: Vec<&str> = with_beta["data"]["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(subcategory_names, vec!["Alpha", "Beta"]);

    let (status, duplicate_sub) = send_authed(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Beta" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(duplicate_sub["code"], "SUBCATEGORY_ALREADY_EXISTS");

    send(
        &app,
        "DELETE",
        &format!("/api/inventory/products/{product_id}"),
        None,
    )
    .await;

    let (status, subcategory_removed) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}/subcategories/{alpha_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let remaining_names: Vec<&str> = subcategory_removed["data"]["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(remaining_names, vec!["Beta"]);

    let (status, deleted) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{category_key}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["data"]["name"], renamed);

    let (status, missing) = send(
        &app,
        "GET",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "CATEGORY_NOT_FOUND");
}

#[tokio::test]
async fn category_field_validation() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);

    // 1. POST with empty icon -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Empty Icon Cat {}", Uuid::new_v4()),
            "icon": "   ",
            "color": "blue",
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 2. POST with empty color -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Empty Color Cat {}", Uuid::new_v4()),
            "icon": "DeviceLaptop",
            "color": "   ",
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 3. POST with valid icon & color -> 201 CREATED
    let (status, created) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Valid Category {}", Uuid::new_v4()),
            "icon": "DeviceLaptop",
            "color": "indigo",
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let category_key = created["data"]["key"].as_str().unwrap().to_string();

    // 4. PUT with empty icon -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "icon": "" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 5. PUT with empty color -> 400 VALIDATION_ERROR
    let (status, err) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "color": "   " })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["code"], "VALIDATION_ERROR");

    // 6. PUT with valid custom icon & color -> 200 OK
    let (status, updated) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/inventory/categories/{category_key}"),
        Some(json!({ "icon": "BrandGithub", "color": "custom-theme-color" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["data"]["icon"], "BrandGithub");
    assert_eq!(updated["data"]["color"], "custom-theme-color");
}

#[tokio::test]
async fn inventory_overview_endpoint_returns_metrics_hierarchical_data_and_filters() {
    let app = common::spawn_app().await;

    // 1. Seed test categories and subcategories
    let (cat1_key, subcat1_key) = seed_category_with_subcategory(&app.db, "LED Strips").await;
    let subcat2_key = seed_subcategory(&app.db, &cat1_key, "Smart Watches").await;

    // 2. Seed products with different stock levels
    // Normal stock product under LED Strips
    let (status, _p1) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "RGB LED Strip Light",
            "categoryKey": cat1_key,
            "subcategoryKey": subcat1_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 1800,
            "stockQuantity": 50,
            "minStockThreshold": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Low stock product under LED Strips (stock 5 <= threshold 10)
    let (status, _p2) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Smart RGB Corner Floor Lamp",
            "categoryKey": cat1_key,
            "subcategoryKey": subcat1_key,
            "costPriceCents": 3000,
            "sellingPriceCents": 6000,
            "stockQuantity": 5,
            "minStockThreshold": 10
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Normal stock product under Smart Watches
    let (status, _p3) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Ultra Smartwatch Screen Protector",
            "categoryKey": cat1_key,
            "subcategoryKey": subcat2_key,
            "costPriceCents": 200,
            "sellingPriceCents": 500,
            "stockQuantity": 20,
            "minStockThreshold": 5
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 3. Test GET /api/inventory/overview without filters
    let (status, overview) = send(&app, "GET", "/api/inventory/overview", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["success"], true);

    let metrics = &overview["data"]["metrics"];
    assert!(metrics["totalItems"].as_u64().unwrap() >= 3);
    assert!(metrics["totalCategories"].as_u64().unwrap() >= 1);
    assert!(metrics["totalSubcategories"].as_u64().unwrap() >= 2);
    assert!(metrics["lowStockAlerts"].as_u64().unwrap() >= 1);

    let categories = overview["data"]["categories"].as_array().unwrap();
    let cat1 = categories
        .iter()
        .find(|c| c["key"] == cat1_key)
        .expect("category 1 should be in overview");
    assert!(cat1["totalItems"].as_u64().unwrap() >= 3);
    assert!(cat1["subcategoriesCount"].as_u64().unwrap() >= 2);

    let subcats = cat1["subcategories"].as_array().unwrap();
    let subcat1 = subcats
        .iter()
        .find(|s| s["key"] == subcat1_key)
        .expect("subcat 1 should be present");
    assert_eq!(subcat1["totalItems"], 2);
    let products_list = subcat1["products"].as_array().unwrap();
    assert_eq!(products_list.len(), 2);
    assert_eq!(subcat1["pagination"]["limit"], 10);

    // 4. Test GET /api/inventory/overview?lowStock=true
    let (status, low_stock_overview) =
        send(&app, "GET", "/api/inventory/overview?lowStock=true", None).await;
    assert_eq!(status, StatusCode::OK);
    let low_stock_cat1 = low_stock_overview["data"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == cat1_key)
        .unwrap();
    let low_stock_subcat1 = low_stock_cat1["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == subcat1_key)
        .unwrap();
    assert_eq!(low_stock_subcat1["totalItems"], 1);
    assert_eq!(
        low_stock_subcat1["products"][0]["name"],
        "Smart RGB Corner Floor Lamp"
    );

    // 5. Test GET /api/inventory/overview?search=LED
    let (status, search_overview) =
        send(&app, "GET", "/api/inventory/overview?search=LED", None).await;
    assert_eq!(status, StatusCode::OK);
    let search_cat1 = search_overview["data"]["categories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == cat1_key)
        .unwrap();
    let search_subcat1 = search_cat1["subcategories"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["key"] == subcat1_key)
        .unwrap();
    assert_eq!(search_subcat1["totalItems"], 2);
}

// ============================================================================
// Authorization
// ============================================================================
//
// Every route under `/api/inventory` except the module-status stub requires a
// token. These tests exist because the gate is easy to lose: authentication
// is opt-in per handler (a handler that declares no `CurrentUser`/`AdminUser`
// argument is silently public), and these product/stock routes were
// unauthenticated for exactly that reason. Deleting an extractor would still
// compile and still pass every other test in this file, so the assertions
// below are the only thing that would catch it.

/// One representative route per method/permission combination — enough to
/// prove the gate is wired, without restating it for all 19 routes.
const READ_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/inventory/overview"),
    ("GET", "/api/inventory/products"),
    ("GET", "/api/inventory/products/low-stock"),
    ("GET", "/api/inventory/stock-movements"),
    ("GET", "/api/inventory/categories"),
    ("GET", "/api/inventory/categories/valid"),
];

const WRITE_ROUTES: &[(&str, &str)] = &[
    ("POST", "/api/inventory/products"),
    ("DELETE", "/api/inventory/products"),
];

#[tokio::test]
async fn inventory_routes_reject_requests_without_a_token() {
    let app = common::spawn_app().await;

    for (method, uri) in READ_ROUTES.iter().chain(WRITE_ROUTES) {
        let body = (*method != "GET").then(|| json!({ "productIds": [] }));
        let (status, json) = send_anon(&app.router, method, uri, body).await;

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must require a token, got {status}: {json}"
        );
        assert_eq!(json["code"], codes::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
async fn inventory_routes_reject_a_token_signed_with_the_wrong_secret() {
    let app = common::spawn_app().await;

    // Same claims, different signing key — proves the signature is actually
    // verified rather than the payload merely being decoded.
    let mut forged_config = (*app.config).clone();
    forged_config.jwt_secret = format!("not-{}", app.config.jwt_secret);
    let forged = manager_token(&forged_config);

    for (method, uri) in [("GET", "/api/inventory/products"), WRITE_ROUTES[0]] {
        let body = (method != "GET").then(|| json!({}));
        let (status, _) = send_authed(&app.router, method, uri, body, &forged).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }

    let (status, _) = send_authed(
        &app.router,
        "GET",
        "/api/inventory/products",
        None,
        "not-a-jwt-at-all",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn staff_may_read_inventory_but_not_write_it() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);

    // Staff holds `inventory:read`, so every read succeeds.
    for (method, uri) in READ_ROUTES {
        let (status, json) = send_authed(&app.router, method, uri, None, &staff).await;
        assert_eq!(status, StatusCode::OK, "{method} {uri} for staff: {json}");
    }

    // ...but not `inventory:write`, so every mutation is refused. This is the
    // distinction that would collapse if the routes were gated on "any
    // authenticated caller" instead of on a permission.
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Alpha").await;
    let (status, product) = send(
        &app,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": format!("Staff Guard Product {}", Uuid::new_v4()),
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 5,
            "minStockThreshold": 1,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "fixture product: {product}");
    let product_id = product["data"]["id"].as_str().unwrap().to_string();

    let forbidden_writes: Vec<(&str, String, Option<Value>)> = vec![
        (
            "POST",
            "/api/inventory/products".to_string(),
            Some(json!({
                "name": "Nope",
                "categoryKey": category_key,
                "subcategoryKey": subcategory_key,
                "costPriceCents": 100,
                "sellingPriceCents": 200,
                "stockQuantity": 1,
                "minStockThreshold": 1,
            })),
        ),
        (
            "PUT",
            format!("/api/inventory/products/{product_id}"),
            Some(json!({ "sellingPriceCents": 100 })),
        ),
        (
            "DELETE",
            format!("/api/inventory/products/{product_id}"),
            None,
        ),
        (
            "DELETE",
            "/api/inventory/products".to_string(),
            Some(json!({ "productIds": [product_id] })),
        ),
        (
            "PATCH",
            format!("/api/inventory/products/{product_id}/stock"),
            Some(json!({ "delta": 5, "reason": "test" })),
        ),
    ];

    for (method, uri, body) in forbidden_writes {
        let (status, json) = send_authed(&app.router, method, &uri, body, &staff).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{method} {uri} for staff: {json}"
        );
        assert_eq!(json["code"], codes::PERMISSION_DENIED, "{method} {uri}");
    }

    // The product is untouched by the five refused writes.
    let (status, still_there) = send(
        &app,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{still_there}");
    assert_eq!(still_there["data"]["stockQuantity"], 5);
}

#[tokio::test]
async fn module_status_stub_stays_public() {
    let app = common::spawn_app().await;

    // Deliberately unauthenticated: it returns a static string, no business
    // data (see the intentional-public list in `app::build_router`). Pinned
    // here so closing or widening it is a conscious decision rather than a
    // side effect.
    // Nested at "/" under the "/api/inventory" prefix, which axum resolves as
    // the prefix itself — there is no trailing-slash form of this route.
    let (status, json) = send_anon(&app.router, "GET", "/api/inventory", None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["success"], true);
}

#[tokio::test]
async fn inventory_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send_anon(&app.router, "GET", "/api/inventory/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Inserts a minimal `ProductDocument` directly, so the stats test can
/// control `stock_quantity`/`min_stock_threshold`/`deleted_at` precisely.
async fn seed_product_with(
    db: &mongodb::Database,
    category_key: &str,
    subcategory_key: &str,
    stock_quantity: i64,
    min_stock_threshold: i64,
    deleted: bool,
) {
    let key = generate_id("prd");
    let now = BsonDateTime::now();
    db.collection::<ProductDocument>("products")
        .insert_one(ProductDocument {
            id: None,
            key,
            sku: format!("SKU-{}", Uuid::new_v4().simple()),
            barcode: None,
            barcode_source: None,
            name: "Stats Test Product".to_string(),
            category_key: category_key.to_string(),
            subcategory_key: subcategory_key.to_string(),
            cost_price_cents: 500,
            selling_price_cents: 1000,
            stock_quantity,
            min_stock_threshold,
            is_serialized: false,
            warranty_months: None,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: if deleted { Some(now) } else { None },
            updated_by_device: None,
        })
        .await
        .expect("failed to seed product");
}

#[tokio::test]
async fn inventory_stats_endpoint_returns_totals_and_low_stock_alerts() {
    // `spawn_app`'s DB is shared across the suite, and `/inventory/stats`
    // aggregates the WHOLE products/categories/subcategories collections
    // with no filter — connect an isolated throwaway database instead, same
    // as the billing/repairs/print-jobs stats tests.
    let app = common::spawn_app().await;
    let db_name = format!("jtstats_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let db = jana2u_pos_backend::clients::mongo::connect(&app.config.mongodb_uri, &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    let (category_key, subcategory_key) = seed_category_with_subcategory(&db, "Widgets").await;
    seed_subcategory(&db, &category_key, "Gadgets").await;
    let (category_key_2, subcategory_key_2) = seed_category_with_subcategory(&db, "Parts").await;

    // 2 products at/below threshold (low stock), 1 comfortably above, 1
    // soft-deleted (must be excluded entirely from both counts).
    seed_product_with(&db, &category_key, &subcategory_key, 2, 5, false).await;
    seed_product_with(&db, &category_key, &subcategory_key, 5, 5, false).await;
    seed_product_with(&db, &category_key_2, &subcategory_key_2, 50, 5, false).await;
    seed_product_with(&db, &category_key_2, &subcategory_key_2, 1, 5, true).await;

    let stats = jana2u_pos_backend::modules::inventory::service::stats::get_inventory_stats(&db)
        .await
        .expect("get_inventory_stats should succeed");

    assert_eq!(
        stats.total_items, 3,
        "the soft-deleted product must be excluded"
    );
    assert_eq!(stats.low_stock_alerts, 2);
    assert_eq!(stats.total_categories, 2);
    assert_eq!(stats.total_subcategories, 3);

    db.drop().await.ok();
}
