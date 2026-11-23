//! Listing invoices by the repair ticket they billed, on SQLite (lines are JSON text there).

use axum::http::StatusCode;
use serde_json::json;

use crate::helpers::{admin_token, send_authed, setup_sqlite_app};

#[tokio::test]
async fn sqlite_lists_invoices_by_the_ticket_they_billed() {
    let ctx = setup_sqlite_app().await;
    let token = admin_token(&ctx.config);

    let (status, customer) = send_authed(
        &ctx.router,
        "POST",
        "/api/customers",
        Some(json!({ "name": "Jane Doe", "primaryPhone": "0771234567" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {customer}");
    let customer_key = customer["data"]["key"].clone();

    let mut ticket_keys = Vec::new();
    for _ in 0..2 {
        let (status, repair) = send_authed(
            &ctx.router,
            "POST",
            "/api/repairs",
            Some(json!({
                "customer": { "customerKey": customer_key },
                "deviceModel": "iPhone 14",
                "issueDescription": "Cracked screen",
                "items": [{ "description": "Repair work", "laborCents": 850000,
                            "warranty": { "coverage": { "kind": "none" } } }],
            })),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "Body: {repair}");
        ticket_keys.push(repair["data"]["key"].clone());
    }

    let (status, sale) = send_authed(
        &ctx.router,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Default Admin" },
            "items": [{
                "name": "Screen Replacement",
                "quantity": 1,
                "discountCents": 0,
                "sourceType": "repair",
                "sourceTicketKey": ticket_keys[0],
            }],
            "payment": { "paymentMethod": "cash", "isCredit": false,
                         "amountReceivedCents": 850000 },
            "shopProfileSnapshot": { "tradingName": "TechFix Repairs" },
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {sale}");
    let invoice_key = sale["data"]["invoice"]["key"].clone();

    let billed_key = ticket_keys[0].as_str().expect("the key is a string");
    let (status, billed) = send_authed(
        &ctx.router,
        "GET",
        &format!("/api/billing/invoices?ticketKey={billed_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {billed}");
    assert_eq!(billed["data"]["invoices"].as_array().map(Vec::len), Some(1));
    assert_eq!(billed["data"]["invoices"][0]["key"], invoice_key);

    let unbilled_key = ticket_keys[1].as_str().expect("the key is a string");
    let (status, unbilled) = send_authed(
        &ctx.router,
        "GET",
        &format!("/api/billing/invoices?ticketKey={unbilled_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {unbilled}");
    assert_eq!(unbilled["data"]["invoices"], json!([]));
}
