//! Shared setup for the `credit_notes_test` tests.

use crate::common;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use mongodb::bson::{DateTime as BsonDateTime, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::id::generate_id;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::billing::model::InvoiceDocument;
use simplebash_pos_backend::modules::inventory::model::{CategoryDocument, SubcategoryDocument};
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
            warranty_template: None,
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

pub(crate) async fn seed_product(
    app: &common::TestApp,
    name: &str,
    price_cents: i64,
    stock: i64,
) -> (String, String, i64) {
    let (category_key, subcategory_key) =
        seed_category_with_subcategory(app.mongo(), "Widgets").await;
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": name,
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "warranty": { "coverage": { "kind": "none" } },
            "openingStock": { "quantity": stock, "unitCostCents": price_cents / 2, "sellingPriceCents": price_cents },
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

/// Completes a simple single-line retail cash sale and returns the invoice key.
pub(crate) async fn complete_simple_sale(
    app: &common::TestApp,
    token: &str,
    product_key: &str,
    quantity: i64,
    unit_price_cents: i64,
    customer_key: Option<&str>,
) -> String {
    let mut body = json!({
        "staff": { "cashierName": "Cashier Alice" },
        "items": [{
            "productKey": product_key,
            "sellingPriceCents": unit_price_cents,
            "quantity": quantity,
            "discountCents": 0,
            "sourceType": "retail"
        }],
        "payment": {
            "paymentMethod": "cash",
            "isCredit": false,
            "amountReceivedCents": unit_price_cents * quantity
        },
        "shopProfileSnapshot": { "tradingName": "Tech Shop" }
    });
    if let Some(customer_key) = customer_key {
        body["customer"] = json!({ "customerKey": customer_key });
    }
    let (status, resp) =
        send_authed(&app.router, "POST", "/api/billing/sales", Some(body), token).await;
    assert_eq!(status, StatusCode::OK, "Sale body: {resp}");
    resp["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string()
}

/// Directly backdates an already-created invoice's `created_at` in Mongo,
/// bypassing the service layer — the only way to simulate "this sale
/// happened outside the return window" without waiting real days.
pub(crate) async fn backdate_invoice(db: &mongodb::Database, invoice_key: &str, days_ago: i64) {
    let backdated =
        BsonDateTime::from_chrono(chrono::Utc::now() - chrono::Duration::days(days_ago));
    db.collection::<InvoiceDocument>("invoices")
        .update_one(
            doc! { "key": invoice_key },
            doc! { "$set": { "created_at": backdated } },
        )
        .await
        .expect("failed to backdate invoice");
}
