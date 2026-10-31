//! Shared setup for the `billing_test` tests.

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

pub(crate) async fn send_anon(
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

pub(crate) fn staff_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

pub(crate) fn admin_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

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
            color: "blue".to_string(),
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

/// Creates a product with 10 units of stock via the real API (Admin token,
/// since inventory writes require `inventory:write`) and returns its
/// `(id, key, sellingPriceCents)`.
pub(crate) async fn seed_product(app: &common::TestApp) -> (String, String, i64) {
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Test Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "openingStock": { "quantity": 10, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "minStockThreshold": 2,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    (
        body["data"]["id"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        body["data"]["key"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        body["data"]["priceTiers"][0]["sellingPriceCents"]
            .as_i64()
            .expect("the value is an integer"),
    )
}

pub(crate) async fn get_product(app: &common::TestApp, id: &str) -> Value {
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"].clone()
}

pub(crate) async fn seed_customer(app: &common::TestApp) -> (String, String) {
    let token = staff_token(&app.config);
    let unique_phone = format!(
        "077{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_millis()
            % 10_000_000
    );
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({ "name": "Kasun Silva", "primaryPhone": unique_phone })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
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

/// Nested request shape: `staff{cashierName}`, `customer{}` (omitted here —
/// a walk-in with no linked account), `items[]`, `pricingAdjustments{}`
/// (omitted — no discount), `payment{paymentMethod, isCredit, ...}`, and
/// `shopProfileSnapshot` at the top level. `subtotalCents`/`totalCents`/
/// `changeDueCents` are never sent — always server-computed (see
/// `CreateSaleRequest`'s doc comment). There is no tax concept.
pub(crate) fn base_sale_payload() -> Value {
    json!({
        "staff": { "cashierName": "Nimal Perera" },
        "items": [],
        "payment": { "paymentMethod": "cash", "isCredit": false },
        "shopProfileSnapshot": { "tradingName": "TechFix Repairs" },
    })
}
