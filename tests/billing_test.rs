mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use chrono::Utc;
use simplebash_pos_backend::{
    core::{config::Config, constants::roles, id::generate_id},
    domain::{billing::InvoiceStatus, users::Role},
    modules::{
        billing::model::InvoiceDocument,
        inventory::model::{CategoryDocument, SubcategoryDocument},
        sync::cursor::encode_cursor,
    },
};
use mongodb::bson::DateTime as BsonDateTime;
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

fn staff_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

fn admin_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

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
async fn seed_product(app: &common::TestApp) -> (String, String, i64) {
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Test Widget",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 1000,
            "sellingPriceCents": 2000,
            "stockQuantity": 10,
            "minStockThreshold": 2,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    (
        body["data"]["id"].as_str().unwrap().to_string(),
        body["data"]["key"].as_str().unwrap().to_string(),
        body["data"]["sellingPriceCents"].as_i64().unwrap(),
    )
}

async fn get_product(app: &common::TestApp, id: &str) -> Value {
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

async fn seed_customer(app: &common::TestApp) -> (String, String) {
    let token = staff_token(&app.config);
    let unique_phone = format!(
        "077{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
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
        body["data"]["id"].as_str().unwrap().to_string(),
        body["data"]["key"].as_str().unwrap().to_string(),
    )
}

async fn seed_repair(app: &common::TestApp) -> String {
    let token = staff_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(json!({
            "customer": {
                "customerName": "Kasun Silva",
                "customerPhone": "0771234567",
            },
            "deviceModel": "iPhone 14",
            "issueDescription": "Cracked screen",
            "estimatedCostCents": 850000,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["key"].as_str().unwrap().to_string()
}

async fn seed_repair_without_price(app: &common::TestApp) -> String {
    let token = staff_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(json!({
            "customer": {
                "customerName": "Kasun Silva",
                "customerPhone": "0771234599",
            },
            "deviceModel": "iPhone 14",
            "issueDescription": "Cracked screen",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["key"].as_str().unwrap().to_string()
}

/// Nested request shape: `staff{cashierName}`, `customer{}` (omitted here —
/// a walk-in with no linked account), `items[]`, `pricingAdjustments{}`
/// (omitted — no discount), `payment{paymentMethod, isCredit, ...}`, and
/// `shopProfileSnapshot` at the top level. `subtotalCents`/`totalCents`/
/// `changeDueCents` are never sent — always server-computed (see
/// `CreateSaleRequest`'s doc comment). There is no tax concept.
fn base_sale_payload() -> Value {
    json!({
        "staff": { "cashierName": "Nimal Perera" },
        "items": [],
        "payment": { "paymentMethod": "cash", "isCredit": false },
        "shopProfileSnapshot": { "tradingName": "TechFix Repairs" },
    })
}

#[tokio::test]
async fn unauthenticated_and_unprivileged_requests_are_rejected() {
    let app = common::spawn_app().await;

    let (status, _) = send_anon(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(base_sale_payload()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let unprivileged = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(base_sale_payload()),
        &unprivileged,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Body: {body}");
}

#[tokio::test]
async fn complete_sale_retail_cash_decrements_stock_and_marks_paid() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (product_id, product_key, price) = seed_product(&app).await;

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "productKey": product_key,
        "quantity": 2,
        "discountCents": 0,
        "sourceType": "retail",
    }]);
    payload["payment"]["amountReceivedCents"] = json!(price * 2);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let data = &body["data"];
    assert_eq!(data["invoice"]["status"], "paid");
    assert!(
        data["invoice"]["invoiceNumber"]
            .as_str()
            .unwrap()
            .starts_with("INV-"),
    );
    assert_eq!(data["payments"].as_array().unwrap().len(), 1);
    assert_eq!(data["payments"][0]["amountCents"], price * 2);
    assert_eq!(data["warnings"], json!([]));

    // subtotalCents/totalCents/changeDueCents were never sent — the server
    // must compute them from the resolved items and the (omitted)
    // pricingAdjustments/amountReceived.
    assert_eq!(data["invoice"]["subtotalCents"], price * 2);
    assert_eq!(data["invoice"]["discountType"], "fixed");
    assert_eq!(data["invoice"]["discountValue"], 0.0);
    assert_eq!(data["invoice"]["discountCents"], 0);
    assert_eq!(data["invoice"]["totalCents"], price * 2);
    assert_eq!(data["invoice"]["changeDueCents"], 0);

    // name/sku/unitPriceCents/totalCents were never sent — the server must
    // have resolved them from the product record via `productKey`.
    let item = &data["invoice"]["items"][0];
    assert_eq!(item["name"], "Test Widget");
    assert_eq!(
        item["sku"].as_str().unwrap(),
        get_product(&app, &product_id).await["sku"]
    );
    assert_eq!(item["unitPriceCents"], price);
    assert_eq!(item["totalCents"], price * 2);
    // The product's cost is snapshotted onto the line at sale time (seed_product
    // creates it with costPriceCents 1000) so retail profit reporting is exact.
    assert_eq!(item["unitCostCents"], 1000);

    let product = get_product(&app, &product_id).await;
    assert_eq!(product["stockQuantity"], 8, "stock should decrement by 2");
}

/// Proves the sync registration (`SYNCABLE` in `modules::sync::service`)
/// reads live data through the real `complete_sale` endpoint, not just a
/// hand-seeded Mongo row like `sync_test.rs`'s DTO-parity test does.
#[tokio::test]
async fn complete_sale_invoice_is_immediately_visible_via_sync_changes() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_product_id, product_key, price) = seed_product(&app).await;

    // Anchor to a cursor from just before the sale so the delta feed is
    // scoped to exactly the invoice this test creates.
    let watermark = encode_cursor(Utc::now() - chrono::Duration::seconds(5), "");

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "productKey": product_key,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);
    payload["payment"]["amountReceivedCents"] = json!(price);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let invoice_key = body["data"]["invoice"]["key"].as_str().unwrap().to_string();

    let (status, sync_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/sync/changes?resources=invoices&since={watermark}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {sync_body}");

    let items = sync_body["data"]["changes"]["invoices"]["items"]
        .as_array()
        .expect("invoices items must be an array");
    let item = items
        .iter()
        .find(|item| item["key"] == json!(invoice_key))
        .expect("the invoice created via complete_sale must appear in the sync delta feed");
    assert_eq!(item["status"], "paid");
    assert_eq!(item["totalCents"], price);
}

#[tokio::test]
async fn complete_sale_retail_item_ignores_client_supplied_name_and_price() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_product_id, product_key, price) = seed_product(&app).await;

    // A client sending a stale/wrong name and unitPriceCents alongside a
    // valid productKey must have both overridden by the DB product record,
    // not trusted from the payload.
    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "productKey": product_key,
        "name": "Totally Wrong Name",
        "unitPriceCents": 1,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let item = &body["data"]["invoice"]["items"][0];
    assert_eq!(item["name"], "Test Widget");
    assert_eq!(item["unitPriceCents"], price);
    assert_eq!(item["totalCents"], price);
}

#[tokio::test]
async fn complete_sale_ad_hoc_retail_item_requires_name_and_unit_price() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an item with no productKey must include name/unitPriceCents: {body}"
    );

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Custom Charge",
        "unitPriceCents": 150000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["invoice"]["items"][0]["name"], "Custom Charge");
}

#[tokio::test]
async fn complete_sale_credit_sale_increases_customer_balance_and_stays_pending() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customer"] = json!({ "customerKey": customer_key });
    payload["payment"]["isCredit"] = json!(true);
    payload["items"] = json!([{
        "name": "Ad-hoc repair charge",
        "unitPriceCents": 500000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["invoice"]["status"], "pending");
    assert_eq!(
        body["data"]["payments"],
        json!([]),
        "credit sale records no payment upfront"
    );

    let (status, customer_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{customer_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(customer_body["data"]["outstandingBalanceCents"], 500000);
    assert_eq!(customer_body["data"]["totalPurchasesCents"], 500000);
}

#[tokio::test]
async fn complete_sale_credit_sale_with_deposit_is_partially_paid() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customer"] = json!({ "customerKey": customer_key });
    payload["payment"]["isCredit"] = json!(true);
    payload["payment"]["amountReceivedCents"] = json!(50000);
    payload["payment"]["dueDate"] = json!("2099-01-01");
    payload["items"] = json!([{
        "name": "Ad-hoc repair charge",
        "unitPriceCents": 150000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["invoice"]["status"], "partially_paid");
    assert_eq!(body["data"]["invoice"]["amountReceivedCents"], 50000);
    let payments = body["data"]["payments"].as_array().unwrap();
    assert_eq!(payments.len(), 1, "the deposit is recorded as one payment");
    assert_eq!(payments[0]["amountCents"], 50000);
    assert_eq!(payments[0]["paymentMethod"], "cash");

    let invoice_key = body["data"]["invoice"]["key"].as_str().unwrap().to_string();
    let (_, invoice_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(invoice_body["data"]["status"], "partially_paid");

    let (_, customer_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{customer_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(
        customer_body["data"]["outstandingBalanceCents"], 100000,
        "only the un-deposited remainder is owed"
    );
    assert_eq!(customer_body["data"]["totalPurchasesCents"], 150000);
}

#[tokio::test]
async fn complete_sale_rejects_deposit_at_or_above_total() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customer"] = json!({ "customerKey": customer_key });
    payload["payment"]["isCredit"] = json!(true);
    payload["payment"]["amountReceivedCents"] = json!(150000);
    payload["payment"]["dueDate"] = json!("2099-01-01");
    payload["items"] = json!([{
        "name": "Ad-hoc repair charge",
        "unitPriceCents": 150000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
}

#[tokio::test]
async fn complete_sale_credit_deposit_by_card_records_card_payment_and_invoice_last4() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customer"] = json!({ "customerKey": customer_key });
    payload["payment"]["isCredit"] = json!(true);
    payload["payment"]["amountReceivedCents"] = json!(50000);
    payload["payment"]["depositMethod"] = json!("card");
    payload["payment"]["cardLast4"] = json!("4321");
    payload["payment"]["dueDate"] = json!("2099-01-01");
    payload["items"] = json!([{
        "name": "Ad-hoc repair charge",
        "unitPriceCents": 150000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let payments = body["data"]["payments"].as_array().unwrap();
    assert_eq!(payments.len(), 1);
    assert_eq!(payments[0]["paymentMethod"], "card");
    assert_eq!(body["data"]["invoice"]["cardLast4"], "4321");
}

#[tokio::test]
async fn void_credit_invoice_with_deposit_reverses_only_the_remainder() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let admin = admin_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customer"] = json!({ "customerKey": customer_key });
    payload["payment"]["isCredit"] = json!(true);
    payload["payment"]["amountReceivedCents"] = json!(50000);
    payload["payment"]["dueDate"] = json!("2099-01-01");
    payload["items"] = json!([{
        "name": "Ad-hoc repair charge",
        "unitPriceCents": 150000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, void_body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Customer changed their mind" })),
        &admin,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a deposited credit invoice can still be voided: {void_body}"
    );

    let (_, customer_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{customer_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(customer_body["data"]["outstandingBalanceCents"], 0);
    assert_eq!(customer_body["data"]["totalPurchasesCents"], 0);
}

#[tokio::test]
async fn record_payment_after_deposit_pays_off_to_paid() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customer"] = json!({ "customerKey": customer_key });
    payload["payment"]["isCredit"] = json!(true);
    payload["payment"]["amountReceivedCents"] = json!(50000);
    payload["payment"]["dueDate"] = json!("2099-01-01");
    payload["items"] = json!([{
        "name": "Credit Purchase",
        "unitPriceCents": 150000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let (status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        Some(json!({ "amountCents": 100000, "paymentMethod": "cash" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, invoice_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(invoice_body["data"]["status"], "paid");

    let (_, customer_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{customer_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(customer_body["data"]["outstandingBalanceCents"], 0);
}

#[tokio::test]
async fn complete_sale_split_payment_records_one_payment_per_leg() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["payment"]["paymentMethod"] = json!("split");
    payload["items"] = json!([{
        "name": "Bundle",
        "unitPriceCents": 300000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);
    payload["payment"]["splitPayments"] = json!([
        { "method": "cash", "amountCents": 200000 },
        { "method": "card", "amountCents": 100000, "cardLast4": "4242" },
    ]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let invoice_key = body["data"]["invoice"]["key"].as_str().unwrap();
    assert_eq!(body["data"]["payments"].as_array().unwrap().len(), 2);

    let (status, payments_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        payments_body["data"]["payments"].as_array().unwrap().len(),
        2
    );
}

#[tokio::test]
async fn complete_sale_with_repair_line_marks_ticket_delivered() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let ticket_key = seed_repair(&app).await;

    // unitPriceCents/assignedEmployeeName are deliberately omitted — both
    // must be resolved from the repair ticket (estimatedCostCents 850000,
    // no assigned employee) rather than trusted from the payload.
    // sourceTicketNumber is also omitted — always server-resolved now.
    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Screen Replacement",
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "repair",
        "sourceTicketKey": ticket_key,
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["warnings"], json!([]));

    let item = &body["data"]["invoice"]["items"][0];
    assert_eq!(item["unitPriceCents"], 850000);
    assert_eq!(item["assignedEmployeeName"], Value::Null);

    let (status, repair_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/repairs/{ticket_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repair_body["data"]["status"], "delivered");
    assert_eq!(
        item["sourceTicketNumber"],
        repair_body["data"]["ticketNumber"]
    );
}

#[tokio::test]
async fn complete_sale_with_priceless_repair_line_is_rejected() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let ticket_key = seed_repair_without_price(&app).await;

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Screen Replacement",
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "repair",
        "sourceTicketKey": ticket_key,
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a priceless repair ticket must reject the whole request before any write: {body}"
    );
    assert_eq!(body["code"], "REPAIR_PRICE_REQUIRED");
}

#[tokio::test]
async fn complete_sale_with_unknown_product_key_fails_the_whole_request() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "productKey": "prod_does_not_exist",
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "an unresolvable productKey must reject the whole request before any write: {body}"
    );
}

#[tokio::test]
async fn complete_sale_with_unknown_source_ticket_key_fails_the_whole_request() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "repair",
        "sourceTicketKey": "rep_does_not_exist",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Body: {body}");
}

#[tokio::test]
async fn complete_sale_computes_fixed_discount() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["pricingAdjustments"] = json!({ "discountType": "fixed", "discountValue": 200 });
    payload["payment"]["amountReceivedCents"] = json!(2000);
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let invoice = &body["data"]["invoice"];
    assert_eq!(
        invoice["subtotalCents"], 1000,
        "sum of resolved item totals"
    );
    assert_eq!(invoice["discountType"], "fixed");
    assert_eq!(invoice["discountValue"], 200.0);
    assert_eq!(invoice["discountCents"], 200);
    assert_eq!(
        invoice["totalCents"], 800,
        "subtotalCents(1000) - discountCents(200) — there is no tax"
    );
    assert_eq!(
        invoice["changeDueCents"], 1200,
        "amountReceivedCents(2000) - totalCents(800)"
    );
}

#[tokio::test]
async fn complete_sale_computes_percentage_discount() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["pricingAdjustments"] = json!({ "discountType": "percentage", "discountValue": 10 });
    payload["payment"]["amountReceivedCents"] = json!(1000);
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let invoice = &body["data"]["invoice"];
    assert_eq!(invoice["subtotalCents"], 1000);
    assert_eq!(invoice["discountType"], "percentage");
    assert_eq!(invoice["discountValue"], 10.0);
    assert_eq!(invoice["discountCents"], 100, "10% of subtotalCents(1000)");
    assert_eq!(invoice["totalCents"], 900);
    assert_eq!(invoice["changeDueCents"], 100);
}

#[tokio::test]
async fn complete_sale_rejects_invalid_pricing_adjustments() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let cases = [
        json!({ "discountType": "bogus", "discountValue": 10 }),
        json!({ "discountType": "percentage", "discountValue": 101 }),
        json!({ "discountType": "percentage", "discountValue": -1 }),
        json!({ "discountType": "fixed", "discountValue": -1 }),
    ];

    for adjustments in cases {
        let mut payload = base_sale_payload();
        payload["pricingAdjustments"] = adjustments.clone();
        payload["items"] = json!([{
            "name": "Widget",
            "unitPriceCents": 1000,
            "quantity": 1,
            "discountCents": 0,
            "sourceType": "retail",
        }]);

        let (status, body) = send_authed(
            &app.router,
            "POST",
            "/api/billing/sales",
            Some(payload),
            &token,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "adjustments {adjustments:?} should be rejected — Body: {body}"
        );
    }
}

#[tokio::test]
async fn complete_sale_split_payment_rejects_legs_exceeding_computed_total() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["payment"]["paymentMethod"] = json!("split");
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);
    // Computed total is 1000, but the split legs sum to 1500.
    payload["payment"]["splitPayments"] = json!([
        { "method": "cash", "amountCents": 1000 },
        { "method": "card", "amountCents": 500 },
    ]);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
}

#[tokio::test]
async fn void_invoice_reverses_stock_requires_admin_and_is_idempotent_guarded() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);
    let (product_id, product_key, _price) = seed_product(&app).await;

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "productKey": product_key,
        "quantity": 3,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &staff,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let product_after_sale = get_product(&app, &product_id).await;
    assert_eq!(product_after_sale["stockQuantity"], 7);

    // Staff cannot void — voiding is Admin-only (D8).
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Customer changed their mind" })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Body: {body}");

    // A blank reason is rejected even for an Admin.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "  " })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");

    let (status, void_body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Customer changed their mind" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {void_body}");
    assert_eq!(void_body["data"]["status"], "voided");

    let product_after_void = get_product(&app, &product_id).await;
    assert_eq!(
        product_after_void["stockQuantity"], 10,
        "voiding must restore the stock that was decremented"
    );

    // Voiding an already-voided invoice is rejected.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Trying again" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {body}");
    assert_eq!(body["code"], "INVOICE_ALREADY_VOIDED");
}

#[tokio::test]
async fn record_payment_against_credit_invoice_pays_it_off_over_installments() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customer"] = json!({ "customerKey": customer_key });
    payload["payment"]["isCredit"] = json!(true);
    payload["items"] = json!([{
        "name": "Credit Purchase",
        "unitPriceCents": 1000000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(sale_body["data"]["invoice"]["status"], "pending");

    let (status, first_payment) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        Some(json!({ "amountCents": 400000, "paymentMethod": "cash" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {first_payment}");

    let (status, invoice_after_first) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        invoice_after_first["data"]["status"], "partially_paid",
        "still owes 600000 cents"
    );

    let (status, second_payment) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        Some(json!({ "amountCents": 600000, "paymentMethod": "cash" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {second_payment}");

    let (status, invoice_after_second) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invoice_after_second["data"]["status"], "paid");

    // A payment beyond the outstanding balance is rejected.
    let (status, over_payment) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        Some(json!({ "amountCents": 1, "paymentMethod": "cash" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {over_payment}");
    assert_eq!(over_payment["code"], "PAYMENT_EXCEEDS_BALANCE");
}

#[tokio::test]
async fn get_invoice_document_returns_pdf_and_caches_by_paper_width() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/pdf")
    );
    let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    // Second call against the same document type is served from cache.
    let cached_req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let cached_resp = app.router.clone().oneshot(cached_req).await.unwrap();
    assert_eq!(cached_resp.status(), StatusCode::OK);
    let cached_bytes = axum::body::to_bytes(cached_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(cached_bytes, pdf_bytes);

    // Thermal receipt with specific paper width works.
    let thermal_req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/thermal-receipt?paperWidthMm=58"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let thermal_resp = app.router.clone().oneshot(thermal_req).await.unwrap();
    assert_eq!(thermal_resp.status(), StatusCode::OK);
    assert_eq!(
        thermal_resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("application/pdf")
    );
}

/// A shop that has a logo (a base64 `data:` URI in its profile snapshot)
/// still renders its A4 invoice — the builder forwards `logoBase64` as
/// `logoUrl`, which passes pre-validation and reaches document-server (the
/// data-URI decode itself is covered in that repo's tests).
#[tokio::test]
async fn get_invoice_document_with_shop_logo_renders() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Widget", "unitPriceCents": 1000, "quantity": 1,
        "discountCents": 0, "sourceType": "retail",
    }]);
    payload["shopProfileSnapshot"] = json!({
        "tradingName": "TechFix Repairs",
        "logoBase64": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    });

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/billing/invoices/{invoice_key}/documents/a4-invoice"
                ))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));
}

/// Inserts a minimal `InvoiceDocument` directly, bypassing `complete_sale`,
/// with full control over the fields the list-filter tests need. Returns
/// `(key, invoiceNumber)` — `invoiceNumber` is uuid-suffixed and unique per
/// call, so it doubles as a search term to scope a query to just this row
/// even against the shared test DB.
#[allow(clippy::too_many_arguments)]
async fn seed_invoice_full(
    db: &mongodb::Database,
    total_cents: i64,
    is_credit: bool,
    status: &str,
    payment_method: &str,
    customer_name: Option<&str>,
    created_at: BsonDateTime,
) -> (String, String) {
    let key = generate_id("inv");
    let invoice_number = format!("INV-{}", Uuid::new_v4().simple());
    db.collection::<InvoiceDocument>("invoices")
        .insert_one(InvoiceDocument {
            id: None,
            key: key.clone(),
            invoice_number: invoice_number.clone(),
            customer_key: None,
            customer_name_snapshot: customer_name.map(|s| s.to_string()),
            customer_phone_snapshot: None,
            customer_address_snapshot: None,
            cashier_id: "staff-1".to_string(),
            cashier_name_snapshot: "Test Cashier".to_string(),
            items: vec![],
            subtotal_cents: total_cents,
            discount_type: "fixed".to_string(),
            discount_value: 0.0,
            discount_cents: 0,
            total_cents,
            payment_method: payment_method.to_string(),
            split_payments: None,
            is_credit,
            amount_received_cents: None,
            change_due_cents: None,
            due_date: None,
            card_last4: None,
            card_ref: None,
            online_ref: None,
            online_note: None,
            status: match status {
                "pending" => InvoiceStatus::Pending,
                "partially_paid" => InvoiceStatus::PartiallyPaid,
                "paid" => InvoiceStatus::Paid,
                "voided" => InvoiceStatus::Voided,
                "closed" => InvoiceStatus::Closed,
                other => panic!("seed_invoice_full: unknown status '{other}'"),
            },
            notes: None,
            shop_profile_snapshot: json!({}),
            warranty_terms_snapshot: None,
            document_selection: None,
            voided_at: None,
            voided_by: None,
            voided_reason: None,
            closed_at: None,
            closed_by: None,
            credit_note_count: 0,
            refunded_cents: 0,
            version: 1,
            created_at,
            updated_at: created_at,
        })
        .await
        .unwrap();
    (key, invoice_number)
}

async fn seed_invoice_with(
    db: &mongodb::Database,
    total_cents: i64,
    is_credit: bool,
    status: &str,
    created_at: BsonDateTime,
) -> String {
    let payment_method = if is_credit { "credit" } else { "cash" };
    let (key, _) = seed_invoice_full(
        db,
        total_cents,
        is_credit,
        status,
        payment_method,
        None,
        created_at,
    )
    .await;
    key
}

#[tokio::test]
async fn billing_stats_endpoint_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send_anon(&app.router, "GET", "/api/billing/invoices/stats", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn billing_stats_endpoint_returns_today_sales_invoice_count_outstanding_credit_and_avg_basket()
 {
    // `spawn_app`'s DB (`MONGODB_TEST_DB_NAME`) is shared across the whole
    // suite, which every other test tolerates by scoping its own queries
    // (a unique search substring, a specific key) — not possible here since
    // the stats endpoint aggregates the WHOLE collection with no filter.
    // So this test connects its own throwaway database, seeds only into
    // that, and calls the service function directly (bypassing HTTP/router
    // entirely) to get exact, uncontaminated numbers.
    let app = common::spawn_app().await;
    // Atlas caps database names at 38 bytes.
    let db_name = format!("jtstats_{}", &Uuid::new_v4().simple().to_string()[..24]);
    let db = simplebash_pos_backend::clients::mongo::connect(&app.config.mongodb_uri, &db_name)
        .await
        .expect("failed to connect to isolated stats test database");

    let now = BsonDateTime::now();
    let yesterday = BsonDateTime::from_chrono(now.to_chrono() - chrono::Duration::days(1));

    // Today: one paid cash sale (2000), one paid credit sale (500 — fully
    // paid, so it does NOT count toward outstanding credit).
    seed_invoice_with(&db, 2000, false, "paid", now).await;
    seed_invoice_with(&db, 500, true, "paid", now).await;
    // Today but pending (not credit) — counts toward outstanding.
    seed_invoice_with(&db, 300, false, "pending", now).await;
    // Backdated (outside "today") paid invoice — must NOT count toward
    // today's sales/count, but a backdated pending one still counts toward
    // the all-time outstanding-credit sum.
    seed_invoice_with(&db, 10_000, false, "paid", yesterday).await;
    seed_invoice_with(&db, 700, false, "pending", yesterday).await;

    let db_handle = simplebash_pos_backend::clients::db::Db::from_mongo(db.clone());
    let stats = simplebash_pos_backend::modules::billing::service::get_billing_stats(&db_handle)
        .await
        .expect("get_billing_stats should succeed");

    // today_sales/count only include the 3 invoices created "now".
    assert_eq!(stats.today_sales_cents, 2000 + 500 + 300);
    assert_eq!(stats.today_invoice_count, 3);
    // outstanding credit = remaining balance on every pending/partially-paid
    // invoice, any date: 300 (today, pending) + 700 (yesterday, pending).
    // The fully-paid credit sale (500) contributes nothing.
    assert_eq!(stats.outstanding_credit_cents, 300 + 700);
    // avg basket = today's sales / today's count, rounded.
    assert_eq!(stats.avg_basket_cents, (2000 + 500 + 300) / 3);

    db.drop().await.ok();
}

#[tokio::test]
async fn list_invoices_filters_by_payment_status_and_payment_method() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let name = format!("PayFilter-{unique}");
    let now = BsonDateTime::now();

    // Paid cash sale.
    let (_, paid_number) =
        seed_invoice_full(&app.db, 1000, false, "paid", "cash", Some(&name), now).await;
    // Paid card sale — same customer name, different payment method.
    let (_, card_number) =
        seed_invoice_full(&app.db, 1500, false, "paid", "card", Some(&name), now).await;
    // Credit sale (is_credit true, still "paid" status) — same customer name.
    let (_, credit_number) =
        seed_invoice_full(&app.db, 2000, true, "paid", "cash", Some(&name), now).await;

    // paymentStatus=paid excludes the credit sale.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&paymentStatus=paid"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let numbers: Vec<String> = body["data"]["invoices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|inv| inv["invoiceNumber"].as_str().unwrap().to_string())
        .collect();
    assert!(numbers.contains(&paid_number));
    assert!(numbers.contains(&card_number));
    assert!(!numbers.contains(&credit_number));

    // paymentStatus=credit returns only the credit sale.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&paymentStatus=credit"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let numbers: Vec<String> = body["data"]["invoices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|inv| inv["invoiceNumber"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(numbers, vec![credit_number]);

    // paymentMethod=card returns only the card sale.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&paymentMethod=card"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let numbers: Vec<String> = body["data"]["invoices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|inv| inv["invoiceNumber"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(numbers, vec![card_number]);
}

#[tokio::test]
async fn list_invoices_filters_by_date_preset() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let name = format!("DatePreset-{unique}");
    let now = BsonDateTime::now();
    let yesterday = BsonDateTime::from_chrono(now.to_chrono() - chrono::Duration::days(1));
    let (_, today_number) =
        seed_invoice_full(&app.db, 1000, false, "paid", "cash", Some(&name), now).await;
    seed_invoice_full(&app.db, 2000, false, "paid", "cash", Some(&name), yesterday).await;

    // No datePreset — search alone finds both.
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["invoices"].as_array().unwrap().len(), 2);

    // datePreset=today narrows to just the one created "now".
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices?search={name}&datePreset=today"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let invoices = body["data"]["invoices"].as_array().unwrap();
    assert_eq!(invoices.len(), 1, "Body: {body}");
    assert_eq!(invoices[0]["invoiceNumber"], today_number);
}

async fn seed_invoice_with_number(
    db: &mongodb::Database,
    invoice_number: &str,
    customer_name: Option<&str>,
) -> String {
    let key = generate_id("inv");
    let now = BsonDateTime::now();
    db.collection::<InvoiceDocument>("invoices")
        .insert_one(InvoiceDocument {
            id: None,
            key: key.clone(),
            invoice_number: invoice_number.to_string(),
            customer_key: None,
            customer_name_snapshot: customer_name.map(|s| s.to_string()),
            customer_phone_snapshot: None,
            customer_address_snapshot: None,
            cashier_id: "staff-1".to_string(),
            cashier_name_snapshot: "Test Cashier".to_string(),
            items: vec![],
            subtotal_cents: 1000,
            discount_type: "fixed".to_string(),
            discount_value: 0.0,
            discount_cents: 0,
            total_cents: 1000,
            payment_method: "cash".to_string(),
            split_payments: None,
            is_credit: false,
            amount_received_cents: None,
            change_due_cents: None,
            due_date: None,
            card_last4: None,
            card_ref: None,
            online_ref: None,
            online_note: None,
            status: InvoiceStatus::Paid,
            notes: None,
            shop_profile_snapshot: json!({}),
            warranty_terms_snapshot: None,
            document_selection: None,
            voided_at: None,
            voided_by: None,
            voided_reason: None,
            closed_at: None,
            closed_by: None,
            credit_note_count: 0,
            refunded_cents: 0,
            version: 1,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
    key
}

#[tokio::test]
async fn list_invoices_searches_by_sequence_number() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    seed_invoice_with_number(&app.db, "INV-000001", Some("Sequence Customer")).await;

    for search_term in ["000001", "1", "00001", "INV-1", "inv-000001"] {
        let (status, body) = send_authed(
            &app.router,
            "GET",
            &format!("/api/billing/invoices?search={search_term}"),
            None,
            &token,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "Failed for search={search_term}: {body}"
        );
        let invoices = body["data"]["invoices"].as_array().unwrap();
        let found = invoices
            .iter()
            .any(|inv| inv["invoiceNumber"] == "INV-000001");
        assert!(
            found,
            "Expected to find INV-000001 when searching for '{search_term}', found: {invoices:?}"
        );
    }
}

// The integration contract: the POS side durably records the
// document-server-minted template key (`tpl_...`) on every generated
// document row — not just the human-readable layout name.
#[tokio::test]
async fn get_invoice_document_persists_document_server_template_key() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let record = app
        .db
        .collection::<mongodb::bson::Document>("generated_documents")
        .find_one(mongodb::bson::doc! { "entity_key": &invoice_key })
        .await
        .expect("query generated_documents")
        .expect("a rendered document should have been recorded");
    assert_eq!(
        record.get_str("template_name").unwrap(),
        "a4-invoice",
        "layout name recorded as before"
    );
    let template_key = record.get_str("template_key").expect(
        "template_key must be recorded on new rows — the POS side holding the \
         document-server's unique ID is the point of this contract",
    );
    // The mock document-server publishes exactly this key for the template.
    assert_eq!(template_key, "tpl_a4_invoice");
}

// Pre-validation: a payload that violates the schema the document-server
// publishes for a template fails HERE with 422 RENDER_VALIDATION_FAILED —
// no wasted round trip, field-level message naming the violated contract.
// Uses its own strict-schema mock so other tests' happy paths are untouched.
#[tokio::test]
async fn document_render_payload_violating_published_schema_fails_fast_locally() {
    let strict_mock_url = common::start_strict_mock_document_server().await;
    let app = common::spawn_app_with_document_server_url(strict_mock_url).await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "sourceType": "retail",
    }]);

    let (_, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(payload),
        &token,
    )
    .await;
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .unwrap()
        .to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["code"], "RENDER_VALIDATION_FAILED");
    let message = body["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("__never_sent_by_backend"),
        "rejection should come from local pre-validation and name the missing \
         required field, got: {message}"
    );
}
