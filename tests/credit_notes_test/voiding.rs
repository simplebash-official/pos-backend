//! Voiding tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use mongodb::bson::doc;
use serde_json::{Value, json};

use tower::ServiceExt;

use crate::common;
use crate::helpers::{
    admin_token, backdate_invoice, complete_simple_sale, execute, get_product, seed_product,
    send_authed, staff_token,
};

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
        .expect("the request is well formed");
    execute(router, request).await
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
    let cn_key = cn_body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

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
    let cn_key = cn_body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

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

// A credit note prints in the shop's design, or in one picked for that single
// print; the choice is part of the cache key so designs never share a PDF.
#[tokio::test]
async fn credit_note_document_accepts_a_design_for_one_print() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (_prod_id, prod_key, price_cents) = seed_product(&app, "Design Test Phone", 5000, 5).await;
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
    let credit_note_key = cn_body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let get = |query: &'static str| {
        let router = app.router.clone();
        let token = token.clone();
        let uri = format!(
            "/api/billing/credit-notes/{credit_note_key}/documents/credit-note?design=current{query}"
        );
        async move {
            let response = router
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(uri)
                        .header(AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())
                        .expect("the request is well formed"),
                )
                .await
                .expect("the awaited step succeeds");
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("the awaited step succeeds");
            (status, String::from_utf8_lossy(&bytes).into_owned())
        }
    };

    let (status, pdf) = get("&template=doc_temp_CompactCredit000").await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.contains("tpl_credit_note_compact"), "{pdf}");

    // The plain print is still the standard design.
    let (status, pdf) = get("").await;
    assert_eq!(status, StatusCode::OK);
    assert!(pdf.contains("tpl_credit_note "), "{pdf}");

    // A design that does not exist (or is for another document) is a clear error.
    for bad in [
        "&template=doc_temp_nope",
        "&template=doc_temp_ZahcoYg8xCQjuc8c",
    ] {
        let (status, body) = get(bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {body}");
        assert!(body.contains("TEMPLATE_UNAVAILABLE"), "{bad}: {body}");
    }
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
    let credit_note_key = cn_body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();

    let request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/credit-notes/{credit_note_key}/documents/credit-note?design=current"
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

    // Unknown documentType is rejected.
    let bad_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/api/billing/credit-notes/{credit_note_key}/documents/return-slip?design=current"
        ))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .expect("the request is well formed");
    let bad_response = app
        .router
        .clone()
        .oneshot(bad_request)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(bad_response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn credit_note_preview_shows_the_refund_the_server_issues_for_a_discounted_line() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    // 3 units at Rs. 4.00 with Rs. 2.00 off the line: Rs. 10.00 net, which is 333 cents a unit
    // to the cent below. Returning 2 units refunds 666 cents, not a prorated 667.
    let (_id, prod_key, _price) = seed_product(&app, "Discounted Line Item", 400, 20).await;
    let (sale_status, sale_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Cashier Alice" },
            "items": [{
                "productKey": prod_key,
                "sellingPriceCents": 400,
                "quantity": 3,
                "discountCents": 200,
                "sourceType": "retail"
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 1000 },
            "shopProfileSnapshot": { "tradingName": "Tech Shop" }
        })),
        &token,
    )
    .await;
    assert_eq!(sale_status, StatusCode::OK, "{sale_body}");
    let invoice_key = sale_body["data"]["invoice"]["key"]
        .as_str()
        .expect("the value is a string");

    let (preview_status, preview) = send_authed(
        &app.router,
        "POST",
        "/api/calc/credit-note",
        Some(json!({
            "invoiceKey": invoice_key,
            "returned": [{ "lineIndex": 0, "quantity": 2 }],
            "noReceipt": null,
            "exchanged": [{ "unitPriceCents": 100, "quantity": 1 }]
        })),
        &token,
    )
    .await;
    assert_eq!(preview_status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["lines"][0]["unitCents"], 333);
    assert_eq!(preview["data"]["returnSubtotalCents"], 666);
    assert_eq!(preview["data"]["exchangeSubtotalCents"], 100);
    assert_eq!(preview["data"]["netRefundCents"], 566);
    assert_eq!(preview["data"]["pastReturnWindow"], false);

    // Too many units, a line that is not there, and both kinds of return at once are refused.
    for bad in [
        json!({ "invoiceKey": invoice_key, "returned": [{ "lineIndex": 0, "quantity": 4 }], "noReceipt": null, "exchanged": [] }),
        json!({ "invoiceKey": invoice_key, "returned": [{ "lineIndex": 9, "quantity": 1 }], "noReceipt": null, "exchanged": [] }),
        json!({ "invoiceKey": invoice_key, "returned": [], "noReceipt": { "unitPriceCents": 1, "quantity": 1 }, "exchanged": [] }),
    ] {
        let (status, _) = send_authed(
            &app.router,
            "POST",
            "/api/calc/credit-note",
            Some(bad),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/calc/credit-note",
        Some(json!({ "invoiceKey": "inv_missing", "returned": [], "noReceipt": null, "exchanged": [] })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The credit note issued for the same return refunds exactly what was previewed.
    let (cn_status, cn_body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": prod_key,
                "quantity": 2,
                "reason": "customer_changed_mind",
                "condition": "resalable"
            }]
        })),
        &token,
    )
    .await;
    assert_eq!(cn_status, StatusCode::OK, "{cn_body}");
    assert_eq!(cn_body["data"]["returnSubtotalCents"], 666);
}

#[tokio::test]
async fn credit_note_preview_flags_a_sale_past_the_return_window() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_id, prod_key, _price) = seed_product(&app, "Old Sale Item", 1000, 5).await;
    let invoice_key = complete_simple_sale(&app, &token, &prod_key, 1, 1000, None).await;
    backdate_invoice(app.mongo(), &invoice_key, 45).await;
    let (status, preview) = send_authed(
        &app.router,
        "POST",
        "/api/calc/credit-note",
        Some(json!({
            "invoiceKey": invoice_key,
            "returned": [{ "lineIndex": 0, "quantity": 1 }],
            "noReceipt": null,
            "exchanged": []
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{preview}");
    assert_eq!(preview["data"]["pastReturnWindow"], true);
    assert!(
        preview["data"]["daysSinceSale"]
            .as_i64()
            .expect("the value is an integer")
            >= 45
    );
    assert_eq!(preview["data"]["returnWindowDays"], 30);
}
