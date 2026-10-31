//! Split payments and stock tests.

use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::{Request, StatusCode};

use serde_json::{Value, json};

use tower::ServiceExt;

use crate::common;
use crate::helpers::{
    admin_token, base_sale_payload, get_product, seed_customer, seed_product, send_authed,
    staff_token,
};

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
    body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string()
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
    body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string()
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
    let invoice_key = body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string");
    assert_eq!(
        body["data"]["payments"]
            .as_array()
            .expect("the value is an array")
            .len(),
        2
    );

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
        payments_body["data"]["payments"]
            .as_array()
            .expect("the value is an array")
            .len(),
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
    let (product_id, product_key, price) = seed_product(&app).await;

    let mut payload = base_sale_payload();
    payload["items"] = json!([{
        "productKey": product_key,
        "sellingPriceCents": price,
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
        .expect("the value is a string")
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
        .expect("the value is a string")
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
        .expect("the value is a string")
        .to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request is well formed");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
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
        .expect("the awaited step succeeds");
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    // Second call against the same document type is served from cache.
    let cached_req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/a4-invoice?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request is well formed");
    let cached_resp = app
        .router
        .clone()
        .oneshot(cached_req)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(cached_resp.status(), StatusCode::OK);
    let cached_bytes = axum::body::to_bytes(cached_resp.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(cached_bytes, pdf_bytes);

    // Thermal receipt with specific paper width works.
    let thermal_req = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/invoices/{invoice_key}/documents/thermal-receipt?paperWidthMm=58&design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request is well formed");
    let thermal_resp = app
        .router
        .clone()
        .oneshot(thermal_req)
        .await
        .expect("the awaited step succeeds");
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
        .expect("the value is a string")
        .to_string();

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/billing/invoices/{invoice_key}/documents/a4-invoice?design=current"
                ))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");
    assert_eq!(response.status(), StatusCode::OK);
    let pdf_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    assert!(pdf_bytes.starts_with(b"%PDF-"));
}
