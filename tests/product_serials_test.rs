mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use simplebash_pos_backend::{
    core::{config::Config, constants::roles},
    domain::users::Role,
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

async fn seed_category_with_subcategory(app: &common::TestApp) -> (String, String) {
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(json!({
            "name": format!("Serial Test Category {}", Uuid::new_v4()),
            "icon": "Box",
            "color": "blue",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    let category_key = body["data"]["key"].as_str().unwrap().to_string();

    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(json!({ "name": "Phones" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    // `POST /categories/{key}/subcategories` returns the whole updated
    // `CategoryInfo` (not the bare new subcategory) — take the last nested
    // subcategory, the one just added.
    let subcategory_key = body["data"]["subcategories"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["key"]
        .as_str()
        .unwrap()
        .to_string();

    (category_key, subcategory_key)
}

/// Creates a serialized product (no initial stock — a serialized product's
/// bundled-supplier-intake path is out of scope, see
/// `service::product::create_product`'s comment) with a warranty period.
async fn seed_serialized_product(
    app: &common::TestApp,
    warranty_months: i64,
) -> (String, String, i64) {
    let (category_key, subcategory_key) = seed_category_with_subcategory(app).await;
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(json!({
            "name": "Serialized Test Phone",
            "categoryKey": category_key,
            "subcategoryKey": subcategory_key,
            "costPriceCents": 30000,
            "sellingPriceCents": 50000,
            "stockQuantity": 0,
            "minStockThreshold": 1,
            "isSerialized": true,
            "warrantyMonths": warranty_months,
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

async fn seed_supplier(app: &common::TestApp) -> String {
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": format!("Serial Test Supplier {}", Uuid::new_v4()),
            "contactPerson": "Jane Supplier",
            "primaryPhone": format!("+9477{:07}", Uuid::new_v4().as_u128() % 10_000_000),
            "address": "123 Test Street, Colombo",
            "suppliedCategories": ["Phones"],
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    body["data"]["key"].as_str().unwrap().to_string()
}

async fn record_purchase(
    app: &common::TestApp,
    supplier_key: &str,
    product_key: &str,
    quantity: i64,
    unit_cost_cents: i64,
    serial_numbers: Option<Vec<String>>,
) -> (StatusCode, Value) {
    let token = admin_token(&app.config);
    let mut body = json!({
        "supplierKey": supplier_key,
        "productKey": product_key,
        "quantity": quantity,
        "unitCostCents": unit_cost_cents,
        "date": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(serials) = serial_numbers {
        body["serialNumbers"] = json!(serials);
    }
    send_authed(&app.router, "POST", "/api/purchases", Some(body), &token).await
}

async fn list_serials(
    app: &common::TestApp,
    product_key: &str,
    status: Option<&str>,
) -> Vec<Value> {
    let token = staff_token(&app.config);
    let uri = match status {
        Some(s) => format!("/api/inventory/products/{product_key}/serials?status={s}"),
        None => format!("/api/inventory/products/{product_key}/serials"),
    };
    let (status_code, body) = send_authed(&app.router, "GET", &uri, None, &token).await;
    assert_eq!(status_code, StatusCode::OK, "Body: {body}");
    body["data"]["items"].as_array().unwrap().clone()
}

fn unique_serials(prefix: &str, count: i64) -> Vec<String> {
    (0..count)
        .map(|i| format!("{prefix}-{}-{}", Uuid::new_v4(), i))
        .collect()
}

#[tokio::test]
async fn purchase_receipt_mints_in_stock_serials_for_a_serialized_product() {
    let app = common::spawn_app().await;
    let (_prod_id, prod_key, _price) = seed_serialized_product(&app, 12).await;
    let supplier_key = seed_supplier(&app).await;

    let serials = unique_serials("MINT", 3);
    let (status, body) = record_purchase(
        &app,
        &supplier_key,
        &prod_key,
        3,
        30000,
        Some(serials.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    let in_stock = list_serials(&app, &prod_key, Some("in_stock")).await;
    assert_eq!(in_stock.len(), 3);
    let minted: Vec<String> = in_stock
        .iter()
        .map(|s| s["serialNumber"].as_str().unwrap().to_string())
        .collect();
    for serial in &serials {
        assert!(minted.contains(serial), "expected {serial} to be minted");
    }
}

#[tokio::test]
async fn purchase_receipt_rejects_wrong_serial_count_for_a_serialized_product() {
    let app = common::spawn_app().await;
    let (_prod_id, prod_key, _price) = seed_serialized_product(&app, 12).await;
    let supplier_key = seed_supplier(&app).await;

    let (status, body) = record_purchase(
        &app,
        &supplier_key,
        &prod_key,
        3,
        30000,
        Some(unique_serials("SHORT", 2)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
}

#[tokio::test]
async fn purchase_receipt_rejects_a_duplicate_serial_number() {
    let app = common::spawn_app().await;
    let (_prod_id, prod_key, _price) = seed_serialized_product(&app, 12).await;
    let supplier_key = seed_supplier(&app).await;

    let dup = unique_serials("DUP", 1);
    let (status, body) =
        record_purchase(&app, &supplier_key, &prod_key, 1, 30000, Some(dup.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    let (status2, body2) =
        record_purchase(&app, &supplier_key, &prod_key, 1, 30000, Some(dup)).await;
    assert_eq!(status2, StatusCode::CONFLICT, "Body: {body2}");
    assert_eq!(body2["code"], "SERIAL_ALREADY_EXISTS");
}

/// Completes a retail sale for a serialized product, providing `serialNumbers`.
async fn complete_serialized_sale(
    app: &common::TestApp,
    token: &str,
    product_key: &str,
    serial_numbers: Vec<String>,
    unit_price_cents: i64,
) -> (StatusCode, Value) {
    let quantity = serial_numbers.len() as i64;
    complete_serialized_sale_with_quantity(
        app,
        token,
        product_key,
        serial_numbers,
        quantity,
        unit_price_cents,
    )
    .await
}

/// Same as `complete_serialized_sale` but lets the caller supply a
/// `quantity` independent of `serial_numbers.len()` — needed to exercise
/// the exact-count validation itself (a mismatch is the point of the test).
async fn complete_serialized_sale_with_quantity(
    app: &common::TestApp,
    token: &str,
    product_key: &str,
    serial_numbers: Vec<String>,
    quantity: i64,
    unit_price_cents: i64,
) -> (StatusCode, Value) {
    let body = json!({
        "staff": { "cashierName": "Cashier Bob" },
        "items": [{
            "productKey": product_key,
            "quantity": quantity,
            "discountCents": 0,
            "sourceType": "retail",
            "serialNumbers": serial_numbers,
        }],
        "payment": {
            "paymentMethod": "cash",
            "isCredit": false,
            "amountReceivedCents": unit_price_cents * quantity
        },
        "shopProfileSnapshot": { "tradingName": "Tech Shop" }
    });
    send_authed(&app.router, "POST", "/api/billing/sales", Some(body), token).await
}

#[tokio::test]
async fn sale_of_serialized_product_requires_exact_serial_count() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let supplier_key = seed_supplier(&app).await;
    let serials = unique_serials("CNT", 2);
    let (status, body) = record_purchase(
        &app,
        &supplier_key,
        &prod_key,
        2,
        30000,
        Some(serials.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    // Sell quantity 2 but supply only 1 serial number.
    let (sale_status, sale_body) = complete_serialized_sale_with_quantity(
        &app,
        &token,
        &prod_key,
        vec![serials[0].clone()],
        2,
        price,
    )
    .await;
    assert_eq!(sale_status, StatusCode::BAD_REQUEST, "Body: {sale_body}");
}

#[tokio::test]
async fn sale_of_serialized_product_rejects_unknown_serial() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;

    let (sale_status, sale_body) = complete_serialized_sale(
        &app,
        &token,
        &prod_key,
        vec!["NOPE-NOT-REAL".to_string()],
        price,
    )
    .await;
    assert_eq!(sale_status, StatusCode::NOT_FOUND, "Body: {sale_body}");
    assert_eq!(sale_body["code"], "SERIAL_NOT_FOUND");
}

#[tokio::test]
async fn sale_of_serialized_product_rejects_an_already_sold_serial() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let supplier_key = seed_supplier(&app).await;
    let serials = unique_serials("RESOLD", 1);
    let (status, body) = record_purchase(
        &app,
        &supplier_key,
        &prod_key,
        1,
        30000,
        Some(serials.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    let (first_status, first_body) =
        complete_serialized_sale(&app, &token, &prod_key, serials.clone(), price).await;
    assert_eq!(first_status, StatusCode::OK, "Body: {first_body}");

    let (second_status, second_body) =
        complete_serialized_sale(&app, &token, &prod_key, serials, price).await;
    assert_eq!(second_status, StatusCode::CONFLICT, "Body: {second_body}");
    assert_eq!(second_body["code"], "SERIAL_ALREADY_SOLD");
}

#[tokio::test]
async fn sale_of_serialized_product_flips_serial_to_sold_with_warranty_expiry() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let supplier_key = seed_supplier(&app).await;
    let serials = unique_serials("SOLD", 1);
    let (status, body) = record_purchase(
        &app,
        &supplier_key,
        &prod_key,
        1,
        30000,
        Some(serials.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");

    let (sale_status, sale_body) =
        complete_serialized_sale(&app, &token, &prod_key, serials.clone(), price).await;
    assert_eq!(sale_status, StatusCode::OK, "Body: {sale_body}");

    let sold = list_serials(&app, &prod_key, Some("sold")).await;
    assert_eq!(sold.len(), 1);
    assert_eq!(sold[0]["serialNumber"], serials[0]);
    assert!(sold[0]["warrantyExpiresAt"].is_string());
    assert_eq!(sold[0]["invoiceKey"], sale_body["data"]["invoice"]["key"]);

    let in_stock = list_serials(&app, &prod_key, Some("in_stock")).await;
    assert!(in_stock.is_empty());
}

/// Sells one serialized unit and returns its invoice key + serial number.
async fn sell_one_serialized_unit(
    app: &common::TestApp,
    token: &str,
    prod_key: &str,
    price: i64,
    warranty_months: i64,
) -> (String, String) {
    let _ = warranty_months;
    let supplier_key = seed_supplier(app).await;
    let serials = unique_serials("RET", 1);
    let (status, body) = record_purchase(
        app,
        &supplier_key,
        prod_key,
        1,
        30000,
        Some(serials.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Body: {body}");
    let (sale_status, sale_body) =
        complete_serialized_sale(app, token, prod_key, serials.clone(), price).await;
    assert_eq!(sale_status, StatusCode::OK, "Body: {sale_body}");
    (
        sale_body["data"]["invoice"]["key"]
            .as_str()
            .unwrap()
            .to_string(),
        serials[0].clone(),
    )
}

#[tokio::test]
async fn credit_note_requires_serial_number_for_a_serialized_line() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let (invoice_key, _serial) = sell_one_serialized_unit(&app, &token, &prod_key, price, 12).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "resalable",
            }],
            "paymentMethod": "cash",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
}

#[tokio::test]
async fn credit_note_resalable_serial_transitions_to_returned_resalable_and_within_warranty() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let (invoice_key, serial) = sell_one_serialized_unit(&app, &token, &prod_key, price, 12).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "resalable",
                "serialNumber": serial,
            }],
            "paymentMethod": "cash",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    assert_eq!(body["data"]["returnedItems"][0]["withinWarranty"], true);

    let resalable = list_serials(&app, &prod_key, Some("returned_resalable")).await;
    assert_eq!(resalable.len(), 1);
    assert_eq!(resalable[0]["serialNumber"], serial);
}

#[tokio::test]
async fn credit_note_damaged_write_off_serial_transitions_to_written_off() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let (invoice_key, serial) = sell_one_serialized_unit(&app, &token, &prod_key, price, 12).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "damaged",
                "disposition": "write_off_scrap",
                "serialNumber": serial,
            }],
            "paymentMethod": "cash",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let written_off = list_serials(&app, &prod_key, Some("written_off")).await;
    assert_eq!(written_off.len(), 1);
    assert_eq!(written_off[0]["serialNumber"], serial);
}

#[tokio::test]
async fn credit_note_damaged_return_to_supplier_serial_transitions_to_under_warranty_claim() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let (invoice_key, serial) = sell_one_serialized_unit(&app, &token, &prod_key, price, 12).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "warranty_claim",
                "condition": "damaged",
                "disposition": "return_to_supplier",
                "serialNumber": serial,
            }],
            "paymentMethod": "cash",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let claims = list_serials(&app, &prod_key, Some("under_warranty_claim")).await;
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["serialNumber"], serial);
}

#[tokio::test]
async fn credit_note_pending_inspection_serial_transitions_to_returned_faulty() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_prod_id, prod_key, price) = seed_serialized_product(&app, 12).await;
    let (invoice_key, serial) = sell_one_serialized_unit(&app, &token, &prod_key, price, 12).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "other",
                "condition": "pending_inspection",
                "serialNumber": serial,
            }],
            "paymentMethod": "cash",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");

    let faulty = list_serials(&app, &prod_key, Some("returned_faulty")).await;
    assert_eq!(faulty.len(), 1);
    assert_eq!(faulty[0]["serialNumber"], serial);
}
