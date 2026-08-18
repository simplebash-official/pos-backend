mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jana2u_pos_backend::{
    core::{config::Config, constants::roles, id::generate_id},
    domain::users::Role,
    modules::inventory::model::{CategoryDocument, SubcategoryDocument},
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
            "customerName": "Kasun Silva",
            "customerPhone": "0771234567",
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

fn base_sale_payload() -> Value {
    json!({
        "cashierName": "Nimal Perera",
        "items": [],
        "subtotalCents": 0,
        "discountCents": 0,
        "taxCents": 0,
        "totalCents": 0,
        "paymentMethod": "cash",
        "isCredit": false,
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
    payload["subtotalCents"] = json!(price * 2);
    payload["totalCents"] = json!(price * 2);
    payload["tenderedAmountCents"] = json!(price * 2);
    payload["changeDueCents"] = json!(0);

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

    let product = get_product(&app, &product_id).await;
    assert_eq!(product["stockQuantity"], 8, "stock should decrement by 2");
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
    payload["subtotalCents"] = json!(price);
    payload["totalCents"] = json!(price);

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
    payload["subtotalCents"] = json!(0);
    payload["totalCents"] = json!(0);

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
    payload["subtotalCents"] = json!(150000);
    payload["totalCents"] = json!(150000);

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
    payload["customerKey"] = json!(customer_key);
    payload["isCredit"] = json!(true);
    payload["items"] = json!([{
        "name": "Ad-hoc repair charge",
        "unitPriceCents": 500000,
        "quantity": 1,
        "discountCents": 0,
        "totalCents": 500000,
        "sourceType": "retail",
    }]);
    payload["subtotalCents"] = json!(500000);
    payload["totalCents"] = json!(500000);

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
async fn complete_sale_split_payment_records_one_payment_per_leg() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["paymentMethod"] = json!("split");
    payload["items"] = json!([{
        "name": "Bundle",
        "unitPriceCents": 300000,
        "quantity": 1,
        "discountCents": 0,
        "totalCents": 300000,
        "sourceType": "retail",
    }]);
    payload["subtotalCents"] = json!(300000);
    payload["totalCents"] = json!(300000);
    payload["splitPayments"] = json!([
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
    payload["subtotalCents"] = json!(850000);
    payload["totalCents"] = json!(850000);

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
    payload["subtotalCents"] = json!(100000);
    payload["totalCents"] = json!(100000);

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
    payload["subtotalCents"] = json!(0);
    payload["totalCents"] = json!(0);

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
async fn complete_sale_rejects_total_that_does_not_match_subtotal_minus_discount_plus_tax() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "name": "Widget",
        "unitPriceCents": 1000,
        "quantity": 1,
        "discountCents": 0,
        "totalCents": 1000,
        "sourceType": "retail",
    }]);
    payload["subtotalCents"] = json!(1000);
    payload["totalCents"] = json!(9999);

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
async fn cancel_invoice_reverses_stock_requires_admin_and_is_idempotent_guarded() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);
    let (product_id, product_key, price) = seed_product(&app).await;

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "productKey": product_key,
        "quantity": 3,
        "discountCents": 0,
        "sourceType": "retail",
    }]);
    payload["subtotalCents"] = json!(price * 3);
    payload["totalCents"] = json!(price * 3);

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

    // Staff cannot cancel — cancellation is Admin-only (D8).
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/cancel"),
        Some(json!({})),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Body: {body}");

    let (status, cancel_body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/cancel"),
        Some(json!({ "reason": "Customer changed their mind" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {cancel_body}");
    assert_eq!(cancel_body["data"]["status"], "cancelled");

    let product_after_cancel = get_product(&app, &product_id).await;
    assert_eq!(
        product_after_cancel["stockQuantity"], 10,
        "cancelling must restore the stock that was decremented"
    );

    // Cancelling an already-cancelled invoice is rejected.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/cancel"),
        Some(json!({})),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "Body: {body}");
    assert_eq!(body["code"], "INVOICE_ALREADY_CANCELLED");
}

#[tokio::test]
async fn record_payment_against_credit_invoice_pays_it_off_over_installments() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_customer_id, customer_key) = seed_customer(&app).await;

    let mut payload = base_sale_payload();
    payload["customerKey"] = json!(customer_key);
    payload["isCredit"] = json!(true);
    payload["items"] = json!([{
        "name": "Credit Purchase",
        "unitPriceCents": 1000000,
        "quantity": 1,
        "discountCents": 0,
        "totalCents": 1000000,
        "sourceType": "retail",
    }]);
    payload["subtotalCents"] = json!(1000000);
    payload["totalCents"] = json!(1000000);

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
        invoice_after_first["data"]["status"], "pending",
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
        "totalCents": 1000,
        "sourceType": "retail",
    }]);
    payload["subtotalCents"] = json!(1000);
    payload["totalCents"] = json!(1000);

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
