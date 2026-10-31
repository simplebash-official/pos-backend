//! Shared setup for the `suppliers_test` tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use mongodb::bson::DateTime as BsonDateTime;
use serde_json::{Value, json};
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::id::generate_id;

use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::inventory::model::{CategoryDocument, SubcategoryDocument};

use crate::common;
use tower::ServiceExt;
use uuid::Uuid;

pub(crate) async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the text is valid JSON")
    };
    (status, json)
}

pub(crate) async fn send(
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
        .expect("the request is well formed");
    execute(router, request).await
}

/// Same as `send`, but with an `Authorization: Bearer <token>` header — for
/// `suppliers`/`supplier_products`/`purchases`, where every route (reads
/// included) requires `AdminUser`.
pub(crate) async fn send_authed(
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
        .expect("the request is well formed");
    execute(router, request).await
}

/// Mints an Admin-role JWT carrying its real permission set. Every
/// `suppliers`/`supplier_products`/`purchases` route requires `AdminUser`
/// (role only), except `POST /api/suppliers`, which needs `suppliers:create`;
/// the inventory API used to seed product fixtures gates on `inventory:write`.
pub(crate) fn admin_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

/// Seeds a category + subcategory directly (bypassing the admin-gated
/// category API), returning `(category_key, subcategory_key)` — the shape
/// `create_product` needs to build a valid `CreateProductRequest`.
pub(crate) async fn seed_category_with_subcategory(
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
pub(crate) async fn create_product(app: &common::TestApp, stock_quantity: i64) -> (String, String) {
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Test Subcategory").await;
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": format!("Test Product {}", Uuid::new_v4()),
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "minStockThreshold": 1,
            "openingStock": (stock_quantity > 0).then(|| json!({
                "quantity": stock_quantity, "unitCostCents": 1000, "sellingPriceCents": 2000,
            })),
        })),
        &admin_token(&app.config),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "failed to create test product: {body}"
    );
    (
        body["data"]["id"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        body["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
    )
}

pub(crate) fn sample_supplier_payload() -> Value {
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
pub(crate) async fn create_supplier(router: &axum::Router, token: &str) -> (String, String, Value) {
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
    let id = body["data"]["id"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    let key = body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    (id, key, body["data"].clone())
}
