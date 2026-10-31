//! Complete sale tests.

use axum::http::StatusCode;

use chrono::Utc;

use serde_json::json;

use simplebash_pos_backend::domain::users::Role;

use simplebash_pos_backend::modules::sync::cursor::encode_cursor;

use crate::common;
use crate::helpers::{
    admin_token, base_sale_payload, get_product, seed_customer, seed_product, send_anon,
    send_authed, staff_token,
};

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
        "sellingPriceCents": price,
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
            .expect("the value is a string")
            .starts_with("INV-"),
    );
    assert_eq!(
        data["payments"]
            .as_array()
            .expect("the value is an array")
            .len(),
        1
    );
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
        item["sku"].as_str().expect("the value is a string"),
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
        "sellingPriceCents": price,
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
    let invoice_key = body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

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
        "sellingPriceCents": price,
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
    let payments = body["data"]["payments"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(payments.len(), 1, "the deposit is recorded as one payment");
    assert_eq!(payments[0]["amountCents"], 50000);
    assert_eq!(payments[0]["paymentMethod"], "cash");

    let invoice_key = body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
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
    let payments = body["data"]["payments"]
        .as_array()
        .expect("the value is an array");
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
        .expect("the value is a string")
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
        .expect("the value is a string")
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
