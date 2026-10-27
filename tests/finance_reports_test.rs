// The money reports: profit and loss, costs by category and month, cash flow per account and what is
// still to pay, worked out from the sales, ledgers and cash movements. On SQLite and on a throwaway Atlas DB.

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

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let request = match body {
        Some(value) => {
            builder = builder.header(CONTENT_TYPE, "application/json");
            builder.body(Body::from(value.to_string())).unwrap()
        }
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.router.clone().oneshot(request).await.unwrap();
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

fn cat(t: CashMovementType, name: &str) -> String {
    category_key(t, name)
}

fn today() -> String {
    chrono::Utc::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn account<'a>(body: &'a Value, kind: &str) -> &'a Value {
    body["data"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == kind)
        .unwrap()
}

fn line_in(account: &Value, code: &str) -> i64 {
    account["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["code"] == code)
        .map_or(0, |l| l["inCents"].as_i64().unwrap())
}

fn line_out(account: &Value, code: &str) -> i64 {
    account["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["code"] == code)
        .map_or(0, |l| l["outCents"].as_i64().unwrap())
}

async fn scenario(app: &common::TestApp) {
    let staff = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let manager = common::mint_token(
        &app.config,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    );
    let period = format!("from={}&to={}", today(), today());

    // Reports are for managers and owners; a cashier is refused, and dates are required and checked.
    for path in ["profit-loss", "expenses", "cash-flow"] {
        let (status, _) = send(
            app,
            "GET",
            &format!("/api/finance-reports/{path}?{period}"),
            None,
            &staff,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
        let (status, _) = send(
            app,
            "GET",
            &format!("/api/finance-reports/{path}"),
            None,
            &manager,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} without dates");
        let (status, _) = send(
            app,
            "GET",
            &format!("/api/finance-reports/{path}?from=2026-02-02&to=2026-02-01"),
            None,
            &manager,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} backwards");
        let (status, _) = send(
            app,
            "GET",
            &format!("/api/finance-reports/{path}?from=2020-01-01&to=2026-02-01"),
            None,
            &manager,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} too long");
    }
    let (status, _) = send(app, "GET", "/api/finance-reports/payables", None, &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A quiet shop: everything is zero.
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/finance-reports/profit-loss?{period}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["netProfitCents"], 0);

    // The same events the accounts test uses: costs from the bank and a card, income into the bank, drawer moves, a card sale.
    let post = |uri: &'static str, value: Value, token: String| async move {
        let (status, body) = send(app, "POST", uri, Some(value), &token).await;
        assert!(
            status == StatusCode::CREATED || status == StatusCode::OK,
            "{uri}: {body}"
        );
    };
    post(
        "/api/expenses",
        json!({ "expenseDate": today(), "categoryKey": cat(CashMovementType::RentAndBills, "Electricity"), "amountCents": 120_000, "paidFrom": "bank" }),
        manager.clone(),
    )
    .await;
    post(
        "/api/expenses",
        json!({ "expenseDate": today(), "categoryKey": cat(CashMovementType::RentAndBills, "Software subscriptions"), "amountCents": 20_000, "paidFrom": "card" }),
        manager.clone(),
    )
    .await;
    post(
        "/api/incomes",
        json!({ "incomeDate": today(), "categoryKey": cat(CashMovementType::AddCash, "Commission received"), "amountCents": 450_000, "receivedInto": "bank" }),
        manager.clone(),
    )
    .await;
    let movement = |t: &str, amount: i64, category: String| json!({ "movementType": t, "amountCents": amount, "terminalId": "term_rep", "categoryKey": category });
    for (t, amount, category) in [
        (
            "add_cash",
            5_000_000,
            cat(CashMovementType::AddCash, "Opening float"),
        ),
        (
            "cash_drop",
            200_000,
            cat(CashMovementType::CashDrop, "To bank deposit"),
        ),
        (
            "cash_drop",
            70_000,
            cat(CashMovementType::CashDrop, "To safe"),
        ),
        (
            "add_cash",
            50_000,
            cat(CashMovementType::AddCash, "Bank withdrawal to drawer"),
        ),
    ] {
        post(
            "/api/sessions/movements",
            movement(t, amount, category),
            staff.clone(),
        )
        .await;
    }
    post(
        "/api/billing/sales",
        json!({
            "staff": { "cashierName": "Nimal" },
            "items": [{ "name": "Repair", "unitPriceCents": 150_000, "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "card", "isCredit": false, "cardLast4": "4242" },
            "shopProfileSnapshot": { "tradingName": "TechFix" }
        }),
        staff.clone(),
    )
    .await;
    // A bill that was due yesterday and is still unpaid.
    let yesterday = (chrono::Utc::now().date_naive() - chrono::Days::new(1))
        .format("%Y-%m-%d")
        .to_string();
    post(
        "/api/expenses",
        json!({ "expenseDate": today(), "categoryKey": cat(CashMovementType::RentAndBills, "Shop rent"), "amountCents": 80_000, "paidFrom": "unpaid", "dueDate": yesterday }),
        manager.clone(),
    )
    .await;

    // Profit and loss: 1,500 sales + 4,500 other income - (1,200 + 200 + 800) costs; the unpaid bill is a cost already.
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/finance-reports/profit-loss?{period}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let pl = &body["data"];
    assert_eq!(pl["salesCents"], 150_000);
    assert_eq!(
        pl["grossProfitCents"],
        150_000 - pl["costOfGoodsCents"].as_i64().unwrap()
    );
    assert_eq!(pl["otherIncomeCents"], 450_000);
    assert_eq!(pl["expensesCents"], 220_000);
    assert_eq!(pl["expenses"][0]["group"], "operating_expense");
    assert_eq!(
        pl["netProfitCents"],
        pl["grossProfitCents"].as_i64().unwrap() + 450_000 - 220_000
    );

    // Costs by category and month: the same 2,200, biggest first, every month of the period present.
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/finance-reports/expenses?{period}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["totalCents"], 220_000);
    assert_eq!(body["data"]["categories"][0]["categoryName"], "Electricity");
    assert_eq!(body["data"]["categories"][0]["totalCents"], 120_000);
    assert_eq!(body["data"]["categories"][0]["sharePercent"], 55);
    assert_eq!(body["data"]["chart"][0]["name"], "Electricity");
    assert_eq!(body["data"]["chart"][0]["isOther"], false);
    assert_eq!(body["data"]["months"].as_array().unwrap().len(), 1);

    // Cash flow per account.
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/finance-reports/cash-flow?{period}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let drawer = account(&body, "drawer");
    assert_eq!(line_in(drawer, "cash_added"), 5_050_000);
    assert_eq!(line_out(drawer, "cash_to_bank_or_safe"), 270_000);
    assert_eq!(drawer["netCents"], 5_050_000 - 270_000);
    let bank = account(&body, "bank");
    assert_eq!(
        line_in(bank, "bank_deposits"),
        200_000,
        "only the bank deposit, not the safe"
    );
    assert_eq!(line_in(bank, "other_income"), 450_000);
    assert_eq!(line_out(bank, "expense_payments"), 120_000);
    assert_eq!(line_out(bank, "bank_withdrawals"), 50_000);
    let card = account(&body, "card");
    assert_eq!(line_in(card, "sales"), 150_000);
    assert_eq!(line_out(card, "expense_payments"), 20_000);
    // A period before today has none of it.
    let (status, body) = send(
        app,
        "GET",
        "/api/finance-reports/cash-flow?from=2026-01-01&to=2026-01-02",
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(account(&body, "bank")["netCents"], 0);

    // Payables: the unpaid, overdue bill.
    let (status, body) = send(app, "GET", "/api/finance-reports/payables", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["billsCents"], 80_000);
    assert_eq!(body["data"]["overdueCents"], 80_000);
    assert_eq!(body["data"]["bills"][0]["isOverdue"], true);
    assert_eq!(
        body["data"]["totalCents"],
        80_000 + body["data"]["suppliersCents"].as_i64().unwrap()
    );
}

#[tokio::test]
async fn the_finance_reports_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn the_finance_reports_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
