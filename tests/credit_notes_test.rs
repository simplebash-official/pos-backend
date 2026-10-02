mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use mongodb::bson::{DateTime as BsonDateTime, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::{config::Config, constants::roles, id::generate_id},
    domain::users::Role,
    modules::{
        billing::model::InvoiceDocument,
        inventory::model::{CategoryDocument, SubcategoryDocument},
    },
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

async fn get_product_movements(app: &common::TestApp, id: &str) -> Vec<Value> {
    let token = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{id}/movements"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    body["data"]["movements"].as_array().unwrap().clone()
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
            "name": "Credit Note Test Customer",
            "primaryPhone": unique_phone,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
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

/// Completes a simple single-line retail cash sale and returns the invoice key.
async fn complete_simple_sale(
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
    resp["data"]["invoice"]["key"].as_str().unwrap().to_string()
}

/// Directly backdates an already-created invoice's `created_at` in Mongo,
/// bypassing the service layer — the only way to simulate "this sale
/// happened outside the return window" without waiting real days.
async fn backdate_invoice(db: &mongodb::Database, invoice_key: &str, days_ago: i64) {
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

#[tokio::test]
async fn credit_note_resalable_restocks_inventory_and_issues_cash_refund() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_id, prod_key, price_cents) = seed_product(&app, "Refund Test Phone", 5000, 10).await;
    let (_cust_id, cust_key) = seed_customer(&app).await;

    let invoice_key =
        complete_simple_sale(&app, &token, &prod_key, 2, price_cents, Some(&cust_key)).await;

    let prod_after_sale = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_sale["stockQuantity"], 8);
    let cust_after_sale = get_customer(&app, &cust_key).await;
    assert_eq!(cust_after_sale["totalPurchasesCents"], price_cents * 2);

    let (cn_status, cn_body) = send_authed(
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
                "notes": "Customer reported minor screen defect"
            }],
            "paymentMethod": "cash",
            "notes": "Refund processed at counter"
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert!(cn_data["key"].as_str().unwrap().starts_with("cn_"));
    assert!(
        cn_data["creditNoteNumber"]
            .as_str()
            .unwrap()
            .starts_with("CN-")
    );
    assert_eq!(cn_data["returnSubtotalCents"], price_cents);
    assert_eq!(cn_data["netRefundCents"], price_cents);
    assert_eq!(cn_data["refundCashCents"], price_cents);
    assert_eq!(cn_data["balanceReductionCents"], 0);
    assert_eq!(cn_data["status"], "resolved");
    assert!(!cn_data["refundPaymentKeys"].as_array().unwrap().is_empty());

    let prod_after_return = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_return["stockQuantity"], 9);
    let cust_after_return = get_customer(&app, &cust_key).await;
    assert_eq!(cust_after_return["totalPurchasesCents"], price_cents);

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
    assert_eq!(inv_data["creditNoteCount"], 1);

    let movements = get_product_movements(&app, &prod_id).await;
    assert!(
        movements
            .iter()
            .any(|m| m["type"] == "return_restock" && m["quantityDelta"] == 1),
        "expected a return_restock +1 movement, got: {movements:?}"
    );
}

#[tokio::test]
async fn credit_note_requires_disposition_when_damaged_and_maps_stock_movement_types() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_id, prod_key, price_cents) = seed_product(&app, "Damaged Cable", 1500, 10).await;
    let invoice_key = complete_simple_sale(&app, &token, &prod_key, 2, price_cents, None).await;

    // Damaged with no disposition must be rejected.
    let (missing_status, missing_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "damaged"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(missing_status, StatusCode::BAD_REQUEST, "{missing_body}");

    // Damaged + write_off_scrap -> ReturnWriteOff, no quantity change, audit-only.
    let (wo_status, wo_body) = send_authed(
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
                "notes": "Cable cut, discarded"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(wo_status, StatusCode::OK, "Credit note body: {wo_body}");

    let prod_after_writeoff = get_product(&app, &prod_id).await;
    assert_eq!(
        prod_after_writeoff["stockQuantity"], 8,
        "write-off must not restock"
    );

    let movements = get_product_movements(&app, &prod_id).await;
    assert!(
        movements
            .iter()
            .any(|m| m["type"] == "return_write_off" && m["quantityDelta"] == 0),
        "expected a return_write_off audit-only movement, got: {movements:?}"
    );

    // Damaged + return_to_supplier -> ReturnSupplierRma, no quantity change, audit-only.
    let (rma_status, rma_body) = send_authed(
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
                "disposition": "return_to_supplier"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(rma_status, StatusCode::OK, "Credit note body: {rma_body}");
    let prod_after_rma = get_product(&app, &prod_id).await;
    assert_eq!(
        prod_after_rma["stockQuantity"], 8,
        "supplier RMA must not restock"
    );
    let movements = get_product_movements(&app, &prod_id).await;
    assert!(
        movements
            .iter()
            .any(|m| m["type"] == "return_supplier_rma" && m["quantityDelta"] == 0),
        "expected a return_supplier_rma audit-only movement, got: {movements:?}"
    );
}

#[tokio::test]
async fn credit_note_with_exchange_items_customer_pays_extra() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_a_id, prod_a_key, prod_a_price) = seed_product(&app, "Product A", 1000, 10).await;
    let (prod_b_id, prod_b_key, _prod_b_price) = seed_product(&app, "Product B", 2500, 10).await;

    let invoice_key = complete_simple_sale(&app, &token, &prod_a_key, 1, prod_a_price, None).await;

    // Return Product A ($10) and exchange for Product B ($25): difference = 1500, customer pays extra.
    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_a_key,
                "quantity": 1,
                "reason": "customer_changed_mind",
                "condition": "resalable"
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
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert_eq!(cn_data["returnSubtotalCents"], 1000);
    assert_eq!(cn_data["exchangeSubtotalCents"], 2500);
    assert_eq!(cn_data["netRefundCents"], -1500);
    assert_eq!(cn_data["exchangeItems"].as_array().unwrap().len(), 1);
    assert!(
        cn_data["exchangeReference"]
            .as_str()
            .unwrap()
            .starts_with("exg_")
    );

    let prod_a = get_product(&app, &prod_a_id).await;
    assert_eq!(prod_a["stockQuantity"], 10);
    let prod_b = get_product(&app, &prod_b_id).await;
    assert_eq!(prod_b["stockQuantity"], 9);
}

#[tokio::test]
async fn credit_note_with_exchange_items_customer_gets_cashback() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_a_id, prod_a_key, _prod_a_price) = seed_product(&app, "Cheap Item", 1000, 10).await;
    let (prod_b_id, prod_b_key, prod_b_price) =
        seed_product(&app, "Expensive Item", 3000, 10).await;

    let invoice_key = complete_simple_sale(&app, &token, &prod_b_key, 1, prod_b_price, None).await;

    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_b_key,
                "quantity": 1,
                "reason": "wrong_item",
                "condition": "resalable"
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
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert_eq!(cn_data["returnSubtotalCents"], 3000);
    assert_eq!(cn_data["exchangeSubtotalCents"], 1000);
    assert_eq!(cn_data["netRefundCents"], 2000);
    assert_eq!(cn_data["refundCashCents"], 2000);

    let prod_b = get_product(&app, &prod_b_id).await;
    assert_eq!(prod_b["stockQuantity"], 10);
    let prod_a = get_product(&app, &prod_a_id).await;
    assert_eq!(prod_a["stockQuantity"], 9);
}

#[tokio::test]
async fn credit_note_refund_is_capped_at_amount_actually_paid_on_partially_paid_invoice() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    // Rs. 250.00 total item, sold on credit (nothing paid at sale time).
    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Credit Sale Item", 25000, 10).await;
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
            "payment": { "paymentMethod": "credit", "isCredit": true },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK, "{sale_body}");
    let invoice_key = sale_body["data"]["invoice"]["key"].as_str().unwrap();

    // Customer pays Rs. 150.00 as a deposit -> PartiallyPaid, Rs. 100.00 still outstanding.
    let (pay_status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/payments"),
        Some(json!({ "amountCents": 15000, "paymentMethod": "cash" })),
        &token,
    )
    .await;
    assert_eq!(pay_status, StatusCode::OK);

    let (inv_status, inv_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(inv_status, StatusCode::OK);
    assert_eq!(inv_body["data"]["status"], "partially_paid");

    // Full return of the item (value Rs. 250.00) — only Rs. 150.00 was actually paid,
    // so refund_cash_cents must cap at 15000 and the remaining 10000 reduces the balance.
    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "customer_changed_mind",
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let cn_data = &cn_body["data"];
    assert_eq!(cn_data["netRefundCents"], price_cents);
    assert_eq!(cn_data["refundCashCents"], 15000);
    assert_eq!(cn_data["balanceReductionCents"], price_cents - 15000);
}

#[tokio::test]
async fn credit_note_refund_breakdown_defaults_proportionally_for_split_paid_invoice() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    // Rs. 100.00 total, split 60/40 cash/card.
    let (_prod_id, prod_key, _price_cents) = seed_product(&app, "Split Pay Item", 10000, 10).await;
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
                "paymentMethod": "split",
                "isCredit": false,
                "splitPayments": [
                    { "method": "cash", "amountCents": 6000 },
                    { "method": "card", "amountCents": 4000 }
                ]
            },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK, "{sale_body}");
    let invoice_key = sale_body["data"]["invoice"]["key"].as_str().unwrap();

    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "customer_changed_mind",
                "condition": "resalable"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let breakdown = cn_body["data"]["refundBreakdown"].as_array().unwrap();
    assert_eq!(breakdown.len(), 2);
    let total: i64 = breakdown
        .iter()
        .map(|l| l["amountCents"].as_i64().unwrap())
        .sum();
    assert_eq!(total, 10000);
    let cash_leg = breakdown.iter().find(|l| l["method"] == "cash").unwrap();
    let card_leg = breakdown.iter().find(|l| l["method"] == "card").unwrap();
    assert_eq!(cash_leg["amountCents"], 6000);
    assert_eq!(card_leg["amountCents"], 4000);
}

#[tokio::test]
async fn credit_note_no_receipt_requires_admin_and_override_reason_priced_at_current_selling_price()
{
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);

    let (_prod_id, prod_key, current_price) = seed_product(&app, "No Receipt Item", 4500, 10).await;

    // Staff cannot even attempt a no-receipt return.
    let (staff_status, staff_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "noReceipt": true,
            "overrideReason": "Customer insists item was bought here",
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &staff,
    )
    .await;
    assert_eq!(staff_status, StatusCode::FORBIDDEN, "{staff_body}");
    assert_eq!(staff_body["code"], "MANAGER_OVERRIDE_REQUIRED");

    // Admin without an override reason is rejected too.
    let (no_reason_status, _) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "noReceipt": true,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &admin,
    )
    .await;
    assert_eq!(no_reason_status, StatusCode::BAD_REQUEST);

    // Admin + override reason succeeds, valued at current selling price.
    let (ok_status, ok_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "noReceipt": true,
            "overrideReason": "Manager verified purchase from receipt-less walk-in",
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &admin,
    )
    .await;
    assert_eq!(ok_status, StatusCode::OK, "{ok_body}");
    let cn_data = &ok_body["data"];
    assert_eq!(cn_data["noReceipt"], true);
    assert_eq!(cn_data["isManagerOverride"], true);
    assert_eq!(cn_data["returnSubtotalCents"], current_price);
    assert!(cn_data["invoiceKey"].is_null());
}

#[tokio::test]
async fn credit_note_past_return_window_is_blocked_unless_manager_overrides() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);

    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Old Sale Item", 3000, 10).await;
    let invoice_key = complete_simple_sale(&app, &staff, &prod_key, 1, price_cents, None).await;
    backdate_invoice(&app.db, &invoice_key, 45).await; // default window is 30 days

    // No override -> blocked.
    let (blocked_status, blocked_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &staff,
    )
    .await;
    assert_eq!(blocked_status, StatusCode::CONFLICT, "{blocked_body}");
    assert_eq!(blocked_body["code"], "RETURN_WINDOW_EXPIRED");

    // Override reason present but caller is Staff, not Admin -> forbidden.
    let (staff_override_status, _) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "overrideReason": "Regular customer, approved verbally",
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }]
        })),
        &staff,
    )
    .await;
    assert_eq!(staff_override_status, StatusCode::FORBIDDEN);

    // Admin + override reason succeeds.
    let (admin_status, admin_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "overrideReason": "Manager approved as a goodwill exception",
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &admin,
    )
    .await;
    assert_eq!(admin_status, StatusCode::OK, "{admin_body}");
    assert_eq!(admin_body["data"]["isManagerOverride"], true);
}

#[tokio::test]
async fn void_credit_note_reverses_restock_payment_and_invoice_counters() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);

    let (prod_id, prod_key, price_cents) = seed_product(&app, "Voidable Item", 2000, 10).await;
    let invoice_key = complete_simple_sale(&app, &staff, &prod_key, 1, price_cents, None).await;

    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "condition": "resalable"
            }],
            "paymentMethod": "cash"
        })),
        &staff,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "{cn_body}");
    let cn_key = cn_body["data"]["key"].as_str().unwrap().to_string();

    let prod_after_cn = get_product(&app, &prod_id).await;
    assert_eq!(prod_after_cn["stockQuantity"], 10);

    // Staff cannot void a credit note.
    let (staff_void_status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/credit-notes/{cn_key}/void"),
        Some(json!({ "reason": "Testing guard" })),
        &staff,
    )
    .await;
    assert_eq!(staff_void_status, StatusCode::FORBIDDEN);

    let (void_status, void_body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/credit-notes/{cn_key}/void"),
        Some(json!({ "reason": "Credit note entered in error" })),
        &admin,
    )
    .await;
    assert_eq!(void_status, StatusCode::OK, "{void_body}");
    assert_eq!(void_body["data"]["status"], "voided");

    let prod_after_void = get_product(&app, &prod_id).await;
    assert_eq!(
        prod_after_void["stockQuantity"], 9,
        "restock must be reversed"
    );

    let (inv_status, inv_body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/billing/invoices/{invoice_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(inv_status, StatusCode::OK);
    assert_eq!(inv_body["data"]["creditNoteCount"], 0);
    assert_eq!(inv_body["data"]["refundedCents"], 0);
}

#[tokio::test]
async fn invoice_close_is_blocked_while_a_credit_note_is_open_and_succeeds_once_voided() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);
    let admin = admin_token(&app.config);

    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Closable Item", 1800, 10).await;
    let invoice_key = complete_simple_sale(&app, &staff, &prod_key, 1, price_cents, None).await;

    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{ "productKey": prod_key, "quantity": 1, "condition": "resalable" }],
            "paymentMethod": "cash"
        })),
        &staff,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "{cn_body}");
    let cn_key = cn_body["data"]["key"].as_str().unwrap().to_string();

    let (blocked_status, blocked_body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/close"),
        None,
        &admin,
    )
    .await;
    assert_eq!(blocked_status, StatusCode::CONFLICT, "{blocked_body}");
    assert_eq!(blocked_body["code"], "INVOICE_NOT_CLOSABLE");

    let (void_status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/credit-notes/{cn_key}/void"),
        Some(json!({ "reason": "Undo for close test" })),
        &admin,
    )
    .await;
    assert_eq!(void_status, StatusCode::OK);

    let (close_status, close_body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/close"),
        None,
        &admin,
    )
    .await;
    assert_eq!(close_status, StatusCode::OK, "{close_body}");
    assert_eq!(close_body["data"]["status"], "closed");
}

#[tokio::test]
async fn credit_note_validates_quantity_and_voided_invoice() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let admin_tok = admin_token(&app.config);

    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Validation Phone", 4000, 10).await;
    let invoice_key = complete_simple_sale(&app, &token, &prod_key, 1, price_cents, None).await;

    // 1. Try to return 2 units (exceeds purchased 1).
    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 2,
                "reason": "defective",
                "condition": "resalable"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::BAD_REQUEST);
    assert_eq!(cn_body["code"], "CREDIT_NOTE_QUANTITY_EXCEEDED");

    // 2. Void the invoice, then a credit note against it must be rejected.
    let (void_status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Testing void guard" })),
        &admin_tok,
    )
    .await;
    assert_eq!(void_status, StatusCode::OK);

    let (cn_status2, cn_body2) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "resalable"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status2, StatusCode::CONFLICT);
    assert_eq!(cn_body2["code"], "INVOICE_NOT_ELIGIBLE_FOR_CREDIT_NOTE");
}

/// Regression test for a real incident: a second credit note against an
/// invoice that the first credit note already fully returned must be
/// rejected, not silently accepted (which would over-refund — two credit
/// notes summing to more than the invoice's own total). This exercises the
/// sequential/deterministic shape of the bug fixed by making the invoice's
/// returned-quantity claim an atomic, version-guarded write that happens
/// before any other durable write in `create_credit_note`.
#[tokio::test]
async fn credit_note_rejects_second_return_after_first_fully_returns_invoice() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (prod_id, prod_key, price_cents) =
        seed_product(&app, "Double Return Phone", 3600, 10).await;
    let invoice_key = complete_simple_sale(&app, &token, &prod_key, 3, price_cents, None).await;

    // First credit note fully returns everything (3 units).
    let (cn1_status, cn1_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 3,
                "reason": "defective",
                "condition": "resalable"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(
        cn1_status,
        StatusCode::OK,
        "first credit note body: {cn1_body}"
    );
    assert_eq!(cn1_body["data"]["refundCashCents"], price_cents * 3);

    // Second credit note against the same invoice, for any quantity at all,
    // must be rejected — nothing is left returnable.
    let (cn2_status, cn2_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 1,
                "reason": "defective",
                "condition": "resalable"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(
        cn2_status,
        StatusCode::BAD_REQUEST,
        "second credit note body: {cn2_body}"
    );
    assert_eq!(cn2_body["code"], "CREDIT_NOTE_QUANTITY_EXCEEDED");

    // The invoice's totals must reflect only the first (successful) credit
    // note — not double-counted, not under-counted.
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
    assert_eq!(inv_data["refundedCents"], price_cents * 3);
    assert_eq!(inv_data["creditNoteCount"], 1);
    assert_eq!(inv_data["items"][0]["returnedQuantity"], 3);

    let prod_after = get_product(&app, &prod_id).await;
    assert_eq!(prod_after["stockQuantity"], 10);
}

#[tokio::test]
async fn void_invoice_requires_reason() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let admin_tok = admin_token(&app.config);

    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Void Guard Item", 900, 5).await;
    let invoice_key = complete_simple_sale(&app, &token, &prod_key, 1, price_cents, None).await;

    let (missing_reason_status, _) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "" })),
        &admin_tok,
    )
    .await;
    assert_eq!(missing_reason_status, StatusCode::BAD_REQUEST);

    let (ok_status, ok_body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/invoices/{invoice_key}/void"),
        Some(json!({ "reason": "Entered by mistake" })),
        &admin_tok,
    )
    .await;
    assert_eq!(ok_status, StatusCode::OK, "{ok_body}");
    assert_eq!(ok_body["data"]["status"], "voided");
}

#[tokio::test]
async fn credit_note_list_get_and_auth_guards() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (anon_status, _) = send_anon(&app.router, "GET", "/api/billing/credit-notes", None).await;
    assert_eq!(anon_status, StatusCode::UNAUTHORIZED);

    let (anon_post_status, _) = send_anon(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({ "invoiceKey": "inv_fake", "returnedItems": [] })),
    )
    .await;
    assert_eq!(anon_post_status, StatusCode::UNAUTHORIZED);

    let (list_status, list_body) = send_authed(
        &app.router,
        "GET",
        "/api/billing/credit-notes?page=1&limit=10",
        None,
        &token,
    )
    .await;
    assert_eq!(list_status, StatusCode::OK, "List body: {list_body}");
    assert!(list_body["data"]["creditNotes"].is_array());
}

#[tokio::test]
async fn credit_note_document_returns_pdf() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (_prod_id, prod_key, price_cents) =
        seed_product(&app, "Document Test Phone", 5000, 5).await;
    let invoice_key = complete_simple_sale(&app, &token, &prod_key, 1, price_cents, None).await;

    let (cn_status, cn_body) = send_authed(
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
            }],
            "paymentMethod": "cash",
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "Credit note body: {cn_body}");
    let credit_note_key = cn_body["data"]["key"].as_str().unwrap().to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/credit-notes/{credit_note_key}/documents/credit-note"
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

    // Unknown documentType is rejected.
    let bad_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/credit-notes/{credit_note_key}/documents/return-slip"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let bad_response = app.router.clone().oneshot(bad_request).await.unwrap();
    assert_eq!(bad_response.status(), StatusCode::BAD_REQUEST);
}
