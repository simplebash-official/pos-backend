//! One record of every money ledger and shop rule, made through the API, for the sync tests.

use axum::body::Body;
use axum::http::Request;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use serde_json::{Value, json};
use simplebash_pos_backend::domain::cash_categories::category_key;
use simplebash_pos_backend::domain::sessions::CashMovementType;
use tower::ServiceExt;

/// The synced resources those records belong to, as `(wire name, table)`.
#[allow(dead_code)]
pub const LEDGER_RESOURCES: [(&str, &str); 10] = [
    ("payees", "payees"),
    ("cashCategories", "cash_categories"),
    ("recurringExpenses", "recurring_expenses"),
    ("incomes", "incomes"),
    ("accountOpenings", "account_openings"),
    ("balanceOpenings", "balance_openings"),
    ("advanceRecoveries", "advance_recoveries"),
    ("attachments", "attachments"),
    ("cashSettings", "cash_settings"),
    ("repairServices", "repair_service_templates"),
];

async fn send(router: &axum::Router, token: &str, method: &str, uri: &str, body: Value) -> Value {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .expect("the request is well formed");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body is read");
    let value: Value = serde_json::from_slice(&bytes).expect("the body is JSON");
    assert!(status.is_success(), "{method} {uri} -> {status}: {value}");
    value
}

fn key_of(response: &Value) -> String {
    response["data"]["key"]
        .as_str()
        .expect("the created record has a key")
        .to_string()
}

/// Creates at least one record of every ledger kind, as the admin `token` signs in. The shop must
/// already have its built-in cash categories.
#[allow(dead_code)]
pub async fn create_ledger_records(router: &axum::Router, token: &str) {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    send(
        router,
        token,
        "POST",
        "/api/cash-categories",
        json!({ "movementType": "petty_expense", "name": "Generator diesel" }),
    )
    .await;
    let payee = send(
        router,
        token,
        "POST",
        "/api/payees",
        json!({ "payeeType": "vendor", "name": "Finance Co" }),
    )
    .await;
    send(
        router,
        token,
        "POST",
        "/api/recurring-expenses",
        json!({
            "categoryKey": category_key(CashMovementType::RentAndBills, "Shop rent"),
            "payeeKey": key_of(&payee), "amountCents": 100_000, "dueDay": 1,
            "remindDaysBefore": 3, "paidFrom": "bank",
        }),
    )
    .await;
    let income = send(
        router,
        token,
        "POST",
        "/api/incomes",
        json!({
            "incomeDate": today,
            "categoryKey": category_key(CashMovementType::AddCash, "Commission received"),
            "amountCents": 450_000, "receivedInto": "bank",
        }),
    )
    .await;
    send(
        router,
        token,
        "POST",
        "/api/attachments",
        json!({
            "ownerKind": "income", "ownerKey": key_of(&income), "fileName": "receipt.jpg",
            "contentType": "image/jpeg",
            "dataBase64": STANDARD.encode([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46]),
        }),
    )
    .await;
    send(
        router,
        token,
        "PUT",
        "/api/accounts/bank/opening",
        json!({ "openingBalanceCents": 1_000, "asOfDate": today }),
    )
    .await;
    let employee = send(
        router,
        token,
        "POST",
        "/api/employees",
        json!({
            "name": "Technician", "phone": "0771234567", "role": "technician",
            "defaultSplitType": "percentage", "defaultSplitValue": 20.0,
        }),
    )
    .await;
    let employee_key = key_of(&employee);
    send(
        router,
        token,
        "PUT",
        &format!("/api/balances/staff/{employee_key}/opening"),
        json!({ "openingBalanceCents": 50_000, "asOfDate": today }),
    )
    .await;
    send(
        router,
        token,
        "POST",
        &format!("/api/balances/staff/{employee_key}/recoveries"),
        json!({ "amountCents": 10_000, "recoveredOn": today, "note": "From pay" }),
    )
    .await;
    send(
        router,
        token,
        "PUT",
        "/api/cash-settings",
        json!({
            "undoWindowSeconds": 60, "approvalThresholdCents": 100_000,
            "quickAmountsCents": [10_000, 50_000],
        }),
    )
    .await;
    send(
        router,
        token,
        "POST",
        "/api/repair-services",
        json!({
            "name": "Screen replacement", "laborCents": 2_500,
            "warranty": { "coverage": { "kind": "none" } }, "active": true,
        }),
    )
    .await;
}
