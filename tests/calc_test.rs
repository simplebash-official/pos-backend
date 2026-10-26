// Live calculations over HTTP. Nothing is stored, so SQLite alone is enough. The request/reply
// pair in `tests/golden/calc/` is the contract the frontend tests replay: this test fails if the
// server's answer ever drifts from it (re-record with `UPDATE_GOLDEN=1 cargo test --test calc_test`).

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::constants::roles,
    domain::{cash_categories::category_key, sessions::CashMovementType, users::Role},
};
use tower::ServiceExt;

async fn post(
    app: &common::TestApp,
    uri: &str,
    body: &Value,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn golden(name: &str, request: &Value, reply: &Value) {
    let path = format!(
        "{}/tests/golden/calc/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let recorded = json!({ "request": request, "reply": reply });
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&recorded).unwrap() + "\n",
        )
        .unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(stored, recorded, "{name} drifted from its golden file");
}

fn receipt_request() -> Value {
    json!({
        "supplierKey": "sup_1", "referenceNo": "INV-1", "date": "2026-10-26",
        "lines": [
            { "productKey": "prod_a", "serialized": false, "quantity": 10, "costMode": "unit",
              "costCents": 5000, "pricingMode": "margin", "sellingPriceCents": 0,
              "marginTenths": 250, "stepCents": 100, "serialNumbers": [] },
            { "productKey": "prod_b", "serialized": false, "quantity": 3, "costMode": "lot",
              "costCents": 1000, "pricingMode": "price", "sellingPriceCents": 500,
              "marginTenths": null, "stepCents": null, "serialNumbers": [] }
        ],
        "charges": [
            { "kind": "shipping", "amountCents": 3000, "billedBy": "third_party",
              "allocation": "value", "description": "" },
            { "kind": "discount", "amountCents": 200, "billedBy": "supplier",
              "allocation": "quantity", "description": "" }
        ],
        "chargesPayment": { "source": "drawer", "paidOn": "" },
        "payment": { "mode": "part", "source": "bank", "partCents": 20000,
                     "paidOn": "2026-10-26", "categoryKey": "ccat_1" }
    })
}

#[tokio::test]
async fn the_receipt_form_is_worked_out_on_the_server() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let request = receipt_request();
    let (status, body) = post(&app, "/api/calc/receipt", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["success"], true);
    golden("receipt", &request, &body["data"]);
}

#[tokio::test]
async fn only_an_admin_may_calculate_a_receipt() {
    let app = common::spawn_app_sqlite().await;
    let request = receipt_request();
    let (status, _) = post(&app, "/api/calc/receipt", &request, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let staff = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = post(&app, "/api/calc/receipt", &request, Some(&staff)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_out_of_range_form_is_refused() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let mut request = receipt_request();
    request["lines"][0]["costCents"] = json!(-5);
    let (status, _) = post(&app, "/api/calc/receipt", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_calculation_is_not_a_write() {
    // No Idempotency-Key is needed or stored: the same body twice answers the same way.
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let request = receipt_request();
    let first = post(&app, "/api/calc/receipt", &request, Some(&admin)).await;
    let second = post(&app, "/api/calc/receipt", &request, Some(&admin)).await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.1["data"], second.1["data"]);
}

fn checkout_request() -> Value {
    json!({
        "items": [
            { "key": "line-1", "unitPriceCents": 490, "quantity": 1, "discount": null, "sourceType": "retail" },
            { "key": "line-2", "unitPriceCents": 650, "quantity": 2, "discount": { "type": "fixed", "value": 100 }, "sourceType": "retail" },
            { "key": "line-3", "unitPriceCents": 1000, "quantity": 3, "discount": { "type": "percentage", "value": 10 }, "sourceType": "retail" },
            { "key": "line-4", "unitPriceCents": 2500, "quantity": 1, "discount": null, "sourceType": "repair" }
        ],
        "orderDiscount": { "type": "percentage", "value": 7.5 },
        "payment": {
            "method": "cash", "tenderedCents": 5000, "splitLegsCents": [],
            "customerBalanceCents": 0, "isCredit": false, "creditDepositCents": 0
        }
    })
}

#[tokio::test]
async fn the_checkout_is_worked_out_on_the_server() {
    let app = common::spawn_app_sqlite().await;
    let token = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let request = checkout_request();
    let (status, body) = post(&app, "/api/calc/checkout", &request, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    // 490 + (1300 - 100) + (3000 - 10% = 2700) + 2500 = 6890; 7.5% is 516.75 -> 517; total 6373.
    assert_eq!(data["subtotalCents"], 6890);
    assert_eq!(data["orderDiscountCents"], 517);
    assert_eq!(data["totalCents"], 6373);
    assert_eq!(data["payment"]["shortByCents"], 1373);
    assert_eq!(data["items"][1]["totalCents"], 1200);
    assert_eq!(data["items"][2]["discountCents"], 300);
    assert_eq!(data["items"][2]["key"], "line-3");
    golden("checkout", &request, data);
}

#[tokio::test]
async fn checkout_needs_a_token_and_the_billing_permission_and_sane_numbers() {
    let app = common::spawn_app_sqlite().await;
    let request = checkout_request();
    let (status, _) = post(&app, "/api/calc/checkout", &request, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = post(&app, "/api/calc/checkout", &request, Some(&nobody)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let token = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let mut huge = checkout_request();
    huge["items"][0]["unitPriceCents"] = json!(i64::MAX);
    let (status, _) = post(&app, "/api/calc/checkout", &huge, Some(&token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let mut negative = checkout_request();
    negative["payment"]["tenderedCents"] = json!(-1);
    let (status, _) = post(&app, "/api/calc/checkout", &negative, Some(&token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

fn quotation_request() -> Value {
    json!({
        "customerKey": "cus_1",
        "customerName": "",
        "lines": [
            { "key": "q1", "name": "Case", "unitPriceCents": 1000, "quantity": 3,
              "discount": { "type": "fixed", "scope": "per_unit", "value": 150 } },
            { "key": "q2", "name": "Cable", "unitPriceCents": 650, "quantity": 1,
              "discount": { "type": "percentage", "scope": "total", "value": 10 } }
        ],
        "orderDiscount": { "type": "percentage", "value": 5 },
        "validUntil": "2099-01-01"
    })
}

#[tokio::test]
async fn the_quotation_is_worked_out_on_the_server() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let request = quotation_request();
    let (status, body) = post(&app, "/api/calc/quotation", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    // 3000 - 450 = 2550; 650 - 65 = 585; subtotal 3135; 5% is 156.75 -> 157; total 2978.
    assert_eq!(data["lines"][0]["discountCents"], 450);
    assert_eq!(data["lines"][1]["totalCents"], 585);
    assert_eq!(data["grossSubtotalCents"], 3650);
    assert_eq!(data["lineDiscountsCents"], 515);
    assert_eq!(data["subtotalCents"], 3135);
    assert_eq!(data["discountCents"], 157);
    assert_eq!(data["totalCents"], 2978);
    assert_eq!(data["blockers"], json!([]));
    golden("quotation", &request, data);
}

#[tokio::test]
async fn a_quotation_calculation_needs_the_quotations_permission() {
    let app = common::spawn_app_sqlite().await;
    let (status, _) = post(&app, "/api/calc/quotation", &quotation_request(), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = post(
        &app,
        "/api/calc/quotation",
        &quotation_request(),
        Some(&nobody),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

fn cash_count_request() -> Value {
    json!({ "counts": { "5000": 1, "1000": 3, "5": 2 } })
}

#[tokio::test]
async fn a_cash_count_is_added_up_on_the_server() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let request = cash_count_request();
    let (status, body) = post(&app, "/api/calc/cash-count", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["totalCents"], 801_000);
    assert_eq!(body["data"]["lines"][0]["id"], "5000");
    assert_eq!(body["data"]["lines"][9]["subtotalCents"], 0);
    golden("cash-count", &request, &body["data"]);
    let (status, _) = post(
        &app,
        "/api/calc/cash-count",
        &json!({ "counts": { "7": 1 } }),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = post(&app, "/api/calc/cash-count", &request, Some(&nobody)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_cash_payment_and_a_shift_count_are_checked_against_the_real_drawer() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let (status, opened) = post(
        &app,
        "/api/sessions/open",
        &json!({ "terminalId": "term_calc", "openingFloatCents": 200_000 }),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let session_key = opened["data"]["key"].as_str().unwrap().to_string();
    let tea = category_key(CashMovementType::PettyExpense, "Tea and refreshments");

    let request = json!({
        "movementType": "petty_expense", "amountCents": 30_000, "categoryKey": tea,
        "sessionKey": session_key, "terminalId": "term_calc", "payeeKey": null,
        "payeeName": "", "period": null, "settlementAmountsCents": [],
        "pinEntered": false, "pinVerified": false, "overrideReasonEntered": false,
        "confirmDuplicate": false
    });
    let (status, body) = post(&app, "/api/calc/cash-movement", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["cashInDrawerCents"], 200_000);
    assert_eq!(data["cashAfterCents"], 170_000);
    assert_eq!(data["overDrawer"], false);
    assert_eq!(data["blockers"], json!([]));
    // The shift key is minted per run; the golden file keeps a fixed one.
    let mut recorded = request.clone();
    recorded["sessionKey"] = json!("ses_golden");
    golden("cash-movement", &recorded, data);

    let mut over = request.clone();
    over["amountCents"] = json!(250_000);
    let (_, body) = post(&app, "/api/calc/cash-movement", &over, Some(&admin)).await;
    assert_eq!(body["data"]["overDrawer"], true);
    assert_eq!(body["data"]["pinRequired"], true);
    assert_eq!(
        body["data"]["blockers"],
        json!(["pin_missing", "override_reason_missing"])
    );

    let close = json!({ "sessionKey": session_key, "terminalId": null, "countedCents": 199_000, "closingFloatCents": 100_000 });
    let (status, body) = post(&app, "/api/calc/shift-count", &close, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["expectedCents"], 200_000);
    assert_eq!(data["varianceCents"], -1_000);
    assert_eq!(data["reasonRequired"], true);
    assert_eq!(data["pinRequired"], true);
    assert_eq!(data["varianceToleranceCents"], 500);
    assert_eq!(data["safeDepositCents"], 99_000);
    assert_eq!(data["keepOpeningFloatCents"], 199_000);
    assert_eq!(data["closingFloatCents"], 100_000);
    let mut recorded = close.clone();
    recorded["sessionKey"] = json!("ses_golden");
    golden("shift-count", &recorded, data);

    let both = json!({ "sessionKey": session_key, "terminalId": "term_calc", "countedCents": 1, "closingFloatCents": null });
    let (status, _) = post(&app, "/api/calc/shift-count", &both, Some(&admin)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    for (uri, body) in [
        ("/api/calc/cash-movement", &request),
        ("/api/calc/shift-count", &close),
    ] {
        let (status, _) = post(&app, uri, body, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let (status, _) = post(&app, uri, body, Some(&nobody)).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn a_jobs_earnings_are_worked_out_on_the_server() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let request = json!({
        "priceCents": 1_000_000, "materialCostCents": 400_000,
        "splitType": "percentage", "splitValue": 30.0
    });
    let (status, body) = post(&app, "/api/calc/earnings", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["profitCents"], 600_000);
    assert_eq!(data["commissionCents"], 180_000);
    assert_eq!(data["shopKeepsCents"], 420_000);
    assert_eq!(data["capped"], false);
    golden("earnings", &request, data);

    let capped = json!({
        "priceCents": 100_000, "materialCostCents": 70_000,
        "splitType": "fixed", "splitValue": 50_000.0
    });
    let (_, body) = post(&app, "/api/calc/earnings", &capped, Some(&admin)).await;
    assert_eq!(body["data"]["commissionCents"], 30_000);
    assert_eq!(body["data"]["capped"], true);

    let bad = json!({ "priceCents": 1, "materialCostCents": 0, "splitType": "percentage", "splitValue": 150.0 });
    let (status, _) = post(&app, "/api/calc/earnings", &bad, Some(&admin)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = post(&app, "/api/calc/earnings", &request, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = post(&app, "/api/calc/earnings", &request, Some(&nobody)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_supplier_payment_is_checked_against_the_suppliers_open_bills() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    // A supplier with no open bills owes nothing, and a bill that is not owed is named.
    let request = json!({
        "supplierKey": "sup_none", "paymentCents": 50_000,
        "settlements": [{ "receiptKey": "prc_missing", "amountCents": 10_000 }]
    });
    let (status, body) = post(&app, "/api/calc/settlement", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["owedCents"], 0);
    assert_eq!(data["oldestFirst"], json!([]));
    assert_eq!(data["blockers"][0]["code"], "bill_not_owed");
    assert_eq!(data["blockers"][0]["receiptKey"], "prc_missing");
    golden("settlement", &request, data);

    let (status, _) = post(&app, "/api/calc/settlement", &request, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = post(&app, "/api/calc/settlement", &request, Some(&nobody)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_invoice_balance_needs_a_real_invoice() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let request = json!({ "invoiceKey": "inv_missing" });
    let (status, _) = post(&app, "/api/calc/invoice-balance", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(&app, "/api/calc/invoice-balance", &request, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bar_lengths_are_worked_out_on_the_server() {
    let app = common::spawn_app_sqlite().await;
    let staff = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let request = json!({ "parts": [
        { "key": "cash", "cents": 30_000, "wholeCents": null },
        { "key": "card", "cents": 10_000, "wholeCents": null },
        { "key": "bill", "cents": 15_000, "wholeCents": 10_000 }
    ]});
    let (status, body) = post(&app, "/api/calc/shares", &request, Some(&staff)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["totalCents"], 55_000);
    assert_eq!(data["parts"][0]["percent"], 55);
    assert_eq!(data["parts"][2]["percent"], 100);
    golden("shares", &request, data);

    let (status, _) = post(&app, "/api/calc/shares", &request, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_totals_over_a_shift_list_are_worked_out_on_the_server() {
    let app = common::spawn_app_sqlite().await;
    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let (status, opened) = post(
        &app,
        "/api/sessions/open",
        &json!({ "terminalId": "term_totals", "openingFloatCents": 100_000 }),
        Some(&admin),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{opened}");
    let key = opened["data"]["key"].as_str().unwrap().to_string();

    let request = json!({ "sessionKeys": [key] });
    let (status, body) = post(&app, "/api/calc/shift-totals", &request, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["shiftCount"], 1);
    assert_eq!(body["data"]["runningCount"], 1);
    assert_eq!(body["data"]["cashSalesCents"], 0);
    assert_eq!(body["data"]["netVarianceCents"], 0);

    let missing = json!({ "sessionKeys": ["ses_missing"] });
    let (status, _) = post(&app, "/api/calc/shift-totals", &missing, Some(&admin)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(&app, "/api/calc/shift-totals", &request, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, _) = post(&app, "/api/calc/shift-totals", &request, Some(&nobody)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
