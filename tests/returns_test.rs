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

async fn seed_product(
    app: &common::TestApp,
    name: &str,
    price_cents: i64,
    stock: i64,
) -> (String, String, i64) {
    let (category_key, subcategory_key) = seed_category_with_subcategory(&app.db, "Widgets").await;
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": name,
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": price_cents / 2,
            "sellingPriceCents": price_cents,
            "stockQuantity": stock,
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
        "+9477{:07}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
            % 10_000_000
    );
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Return Test Customer",
            "primaryPhone": unique_phone,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    (
        body["data"]["id"].as_str().unwrap().to_string(),
        body["data"]["key"].as_str().unwrap().to_string(),
    )
}

async fn get_customer(app: &common::TestApp, key: &str) -> Value {
    let token = staff_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/customers/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"].clone()
}

#[tokio::test]
async fn return_flow_restocks_inventory_and_issues_cash_refund() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_id, prod_key, price_cents) = seed_product(&app, "Refund Test Phone", 5000, 10).await;
    let (_cust_id, cust_key) = seed_customer(&app).await;

    // Complete sale of 2 units
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "customer": { "customerKey": cust_key },
            "items": [{
                "productKey": prod_key,
                "quantity": 2,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": price_cents * 2
            },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK, "Sale body: {sale_body}");
    let invoice_key = sale_body["data"]["invoice"]["key"].as_str().unwrap();

    // Stock should now be 8
    let prod_after_sale = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_sale["stockQuantity"], 8);

    // Customer lifetime spend should be 10000
    let cust_after_sale = get_customer(&app, &cust_key).await;
    assert_eq!(cust_after_sale["totalPurchasesCents"], price_cents * 2);

    // Process return of 1 unit with restock
    let (ret_status, ret_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/returns",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "restockAction": "restock_to_inventory",
                "notes": "Customer reported minor screen defect"
            }],
            "paymentMethod": "cash",
            "notes": "Refund processed at counter"
        })),
        &token,
    )
    .await;
    assert_eq!(ret_status, StatusCode::OK, "Return body: {ret_body}");
    let ret_data = &ret_body["data"];
    assert!(ret_data["key"].as_str().unwrap().starts_with("ret_"));
    assert!(
        ret_data["returnNumber"]
            .as_str()
            .unwrap()
            .starts_with("RET-")
    );
    assert_eq!(ret_data["returnSubtotalCents"], price_cents);
    assert_eq!(ret_data["netRefundCents"], price_cents);
    assert!(ret_data["refundPaymentKey"].is_string());

    // Product stock should be restored to 9
    let prod_after_return = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_return["stockQuantity"], 9);

    // Customer lifetime spend should be reduced by price_cents (5000)
    let cust_after_return = get_customer(&app, &cust_key).await;
    assert_eq!(cust_after_return["totalPurchasesCents"], price_cents);

    // Verify invoice returned quantity and refunded cents
    let (inv_status, inv_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(inv_status, StatusCode::OK);
    let inv_data = &inv_body["data"];
    assert_eq!(inv_data["refundedCents"], price_cents);
    assert_eq!(inv_data["items"][0]["returnedQuantity"], 1);
}

#[tokio::test]
async fn return_flow_damaged_discard_does_not_restock_inventory() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_id, prod_key, price_cents) = seed_product(&app, "Damaged Cable", 1500, 10).await;

    // Complete sale of 1 unit
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "productKey": prod_key,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": price_cents
            },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK);
    let invoice_key = sale_body["data"]["invoice"]["key"].as_str().unwrap();

    let prod_after_sale = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_sale["stockQuantity"], 9);

    // Return with damaged_discard
    let (ret_status, ret_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/returns",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "restockAction": "damaged_discard",
                "notes": "Cable cut, discarded"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(ret_status, StatusCode::OK, "Return body: {ret_body}");

    // Stock should remain 9 (not restocked)
    let prod_after_return = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_return["stockQuantity"], 9);
}

#[tokio::test]
async fn return_flow_with_exchange_items_customer_pays_extra() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_a_id, prod_a_key, prod_a_price) = seed_product(&app, "Product A", 1000, 10).await;
    let (prod_b_id, prod_b_key, _prod_b_price) = seed_product(&app, "Product B", 2500, 10).await;

    // Sale of Product A ($10)
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "productKey": prod_a_key,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": prod_a_price
            },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK);
    let invoice_key = sale_body["data"]["invoice"]["key"].as_str().unwrap();

    // Return Product A ($10) and exchange for Product B ($25)
    // Difference = 2500 - 1000 = 1500 (net_refund_cents = -1500)
    let (ret_status, ret_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/returns",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_a_key,
                "quantity": 1,
                "reason": "customer_changed_mind",
                "restockAction": "restock_to_inventory"
            }],
            "exchangeItems": [{
                "productKey": prod_b_key,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "paymentMethod": "card"
        })),
        &token,
    )
    .await;
    assert_eq!(ret_status, StatusCode::OK, "Return body: {ret_body}");
    let ret_data = &ret_body["data"];
    assert_eq!(ret_data["returnSubtotalCents"], 1000);
    assert_eq!(ret_data["exchangeSubtotalCents"], 2500);
    assert_eq!(ret_data["netRefundCents"], -1500);
    assert_eq!(ret_data["exchangeItems"].as_array().unwrap().len(), 1);

    // Product A stock: originally 10 -> sold 1 (9) -> returned 1 (10)
    let prod_a = get_product(&app, &prod_a_id).await;
    assert_eq!(prod_a["stockQuantity"], 10);

    // Product B stock: originally 10 -> exchanged 1 (9)
    let prod_b = get_product(&app, &prod_b_id).await;
    assert_eq!(prod_b["stockQuantity"], 9);
}

#[tokio::test]
async fn return_flow_with_exchange_items_customer_gets_cashback() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_a_id, prod_a_key, _prod_a_price) = seed_product(&app, "Cheap Item", 1000, 10).await;
    let (prod_b_id, prod_b_key, prod_b_price) =
        seed_product(&app, "Expensive Item", 3000, 10).await;

    // Sale of Product B ($30)
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "productKey": prod_b_key,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": prod_b_price
            },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK);
    let invoice_key = sale_body["data"]["invoice"]["key"].as_str().unwrap();

    // Return Product B ($30) and exchange for Product A ($10)
    // Difference = 3000 - 1000 = 2000 cashback
    let (ret_status, ret_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/returns",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_b_key,
                "quantity": 1,
                "reason": "wrong_item",
                "restockAction": "restock_to_inventory"
            }],
            "exchangeItems": [{
                "productKey": prod_a_key,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(ret_status, StatusCode::OK, "Return body: {ret_body}");
    let ret_data = &ret_body["data"];
    assert_eq!(ret_data["returnSubtotalCents"], 3000);
    assert_eq!(ret_data["exchangeSubtotalCents"], 1000);
    assert_eq!(ret_data["netRefundCents"], 2000);

    // Check stocks
    let prod_b = get_product(&app, &prod_b_id).await;
    assert_eq!(prod_b["stockQuantity"], 10);
    let prod_a = get_product(&app, &prod_a_id).await;
    assert_eq!(prod_a["stockQuantity"], 9);
}

#[tokio::test]
async fn return_validates_quantity_and_cancelled_invoice() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let admin_tok = admin_token(&app.config);

    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Validation Phone", 4000, 10).await;

    // Sale of 1 unit
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "productKey": prod_key,
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "retail"
            }],
            "payment": {
                "paymentMethod": "cash",
                "isCredit": false,
                "amountReceivedCents": price_cents
            },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK);
    let invoice_key = sale_body["data"]["invoice"]["key"].as_str().unwrap();

    // 1. Try to return 2 units (exceeds purchased 1)
    let (ret_status, ret_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/returns",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 2,
                "reason": "defective",
                "restockAction": "restock_to_inventory"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(ret_status, StatusCode::BAD_REQUEST);
    assert_eq!(ret_body["code"], "RETURN_QUANTITY_EXCEEDED");

    // 2. Cancel the invoice then try to return
    let (cancel_status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/cancel"),
        Some(json!({ "reason": "Testing cancellation guard" })),
        &admin_tok,
    )
    .await;
    assert_eq!(cancel_status, StatusCode::OK);

    let (ret_status2, ret_body2) = send_authed(
        &app.router,
        "POST",
        "/api/billing/returns",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "restockAction": "restock_to_inventory"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(ret_status2, StatusCode::CONFLICT);
    assert_eq!(ret_body2["code"], "INVOICE_NOT_RETURNABLE");
}

#[tokio::test]
async fn return_list_get_and_auth_guards() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    // 1. Unauthenticated requests must fail with 401
    let (anon_status, _) = send_anon(&app.router, "GET", "/api/billing/returns", None).await;
    assert_eq!(anon_status, StatusCode::UNAUTHORIZED);

    let (anon_post_status, _) = send_anon(
        &app.router,
        "POST",
        "/api/billing/returns",
        Some(json!({ "invoiceKey": "inv_fake", "returnedItems": [] })),
    )
    .await;
    assert_eq!(anon_post_status, StatusCode::UNAUTHORIZED);

    // 2. List returns
    let (list_status, list_body) = send_authed(
        &app.router,
        "GET",
        "/api/billing/returns?page=1&limit=10",
        None,
        &token,
    )
    .await;
    assert_eq!(list_status, StatusCode::OK, "List body: {list_body}");
    assert!(list_body["data"]["returns"].is_array());
}
