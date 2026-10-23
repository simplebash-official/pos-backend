// The expense ledger: costs from the bank, a card or online, bills still to pay and their
// payments, and drawer payments appearing in the same list. On SQLite and on a throwaway Atlas DB.

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

async fn send(app: &common::TestApp, method: &str, uri: &str, body: Option<Value>, token: &str) -> (StatusCode, Value) {
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
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, json)
}

fn cat(t: CashMovementType, name: &str) -> String {
    category_key(t, name)
}

fn today() -> String {
    chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

async fn scenario(app: &common::TestApp) {
    let staff = common::mint_token(&app.config, Some(Role::Staff), roles::default_permissions(Role::Staff));
    let manager = common::mint_token(&app.config, Some(Role::Manager), roles::default_permissions(Role::Manager));

    // A cashier sees the drawer's movements, not the shop's costs.
    for (method, uri) in [("GET", "/api/expenses"), ("GET", "/api/expenses/stats")] {
        let (status, _) = send(app, method, uri, None, &staff).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    let electricity = cat(CashMovementType::RentAndBills, "Electricity");
    let (status, _) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({ "expenseDate": today(), "categoryKey": electricity, "amountCents": 100, "paidFrom": "bank" })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // A bill paid from the bank is entered once and is paid in full on its date.
    let (status, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": today(), "categoryKey": electricity, "payeeName": "CEB",
            "amountCents": 1_200_000, "paidFrom": "bank", "referenceNo": "BILL-77", "note": "October"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let paid = &body["data"]["expense"];
    assert!(paid["number"].as_str().unwrap().starts_with("EXP-"));
    assert_eq!(paid["status"], "paid");
    assert_eq!(paid["paidFrom"], "bank");
    assert_eq!(paid["balanceCents"], 0);
    assert_eq!(paid["source"], "ledger");
    assert_eq!(paid["categoryName"], "Electricity");
    assert_eq!(body["data"]["payments"].as_array().unwrap().len(), 1);
    assert!(paid["payeeKey"].as_str().unwrap().starts_with("pye_other_"), "the typed name is saved for next time");

    // A bill still to pay, already past its due date.
    let (status, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": "2020-01-05", "categoryKey": electricity, "payeeName": "Water Board",
            "amountCents": 100_000, "paidFrom": "unpaid", "dueDate": "2020-01-20"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let bill_key = body["data"]["expense"]["key"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["expense"]["status"], "unpaid");
    assert_eq!(body["data"]["expense"]["paidFrom"], "unpaid");
    assert_eq!(body["data"]["expense"]["isOverdue"], true);
    assert_eq!(body["data"]["payments"].as_array().unwrap().len(), 0);

    // What can and cannot be entered.
    let bad = |extra: Value| {
        let mut base = json!({ "expenseDate": today(), "categoryKey": electricity, "amountCents": 100, "paidFrom": "bank" });
        base.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        base
    };
    for (what, extra) in [
        ("drawer cash is a cash movement", json!({ "paidFrom": "drawer" })),
        ("no amount", json!({ "amountCents": 0 })),
        ("a made-up date", json!({ "expenseDate": "2026-02-30" })),
        ("a due date on a paid bill", json!({ "dueDate": "2026-12-01" })),
        ("owner drawings are not a cost", json!({ "categoryKey": cat(CashMovementType::OwnerWithdrawal, "Personal use") })),
        ("added cash is not a cost", json!({ "categoryKey": cat(CashMovementType::AddCash, "Owner capital") })),
        ("a loan instalment is not a cost", json!({ "categoryKey": cat(CashMovementType::RentAndBills, "Loan or lease installment") })),
    ] {
        let (status, body) = send(app, "POST", "/api/expenses", Some(bad(extra)), &manager).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
    }

    // Paying a bill in parts, never more than is owed.
    let pay = |amount: i64| json!({ "amountCents": amount, "paidFrom": "online", "paidOn": today(), "referenceNo": "TXN-1" });
    let (status, body) = send(app, "POST", &format!("/api/expenses/{bill_key}/payments"), Some(pay(40_000)), &manager).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["expense"]["status"], "part_paid");
    assert_eq!(body["data"]["expense"]["balanceCents"], 60_000);
    assert_eq!(body["data"]["expense"]["isOverdue"], true, "still overdue until it is paid in full");
    let (status, body) = send(app, "POST", &format!("/api/expenses/{bill_key}/payments"), Some(pay(60_001)), &manager).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "EXPENSE_OVERPAID");
    let (status, _) = send(
        app,
        "POST",
        &format!("/api/expenses/{bill_key}/payments"),
        Some(json!({ "amountCents": 100, "paidFrom": "drawer", "paidOn": today() })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = send(app, "POST", &format!("/api/expenses/{bill_key}/payments"), Some(pay(60_000)), &manager).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["expense"]["status"], "paid");
    assert_eq!(body["data"]["expense"]["isOverdue"], false);
    assert_eq!(body["data"]["payments"].as_array().unwrap().len(), 2);
    let (status, _) = send(app, "GET", "/api/expenses/exp_nope", None, &manager).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Drawer payments show up in the same list without being entered twice. Payouts need cash in the drawer.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash", "amountCents": 10_000_000, "terminalId": "term_exp",
            "categoryKey": cat(CashMovementType::AddCash, "Opening float")
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "petty_expense", "amountCents": 35_000, "terminalId": "term_exp",
            "categoryKey": cat(CashMovementType::PettyExpense, "Tea and refreshments"), "payeeName": "Corner shop"
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let voucher = body["data"]["movementNumber"].as_str().unwrap().to_string();
    // A loan instalment paid from the drawer is not a business cost, so it stays out of the list.
    let (status, vendor) = send(
        app,
        "POST",
        "/api/payees",
        Some(json!({ "payeeType": "vendor", "name": "Finance Co" })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{vendor}");
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "rent_and_bills", "amountCents": 90_000, "terminalId": "term_exp",
            "categoryKey": cat(CashMovementType::RentAndBills, "Loan or lease installment"),
            "payeeKey": vendor["data"]["key"], "periodStart": "2026-10-01", "periodEnd": "2026-10-31"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = send(app, "GET", "/api/expenses?source=drawer", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let drawer = body["data"]["expenses"].as_array().unwrap();
    assert_eq!(drawer.len(), 1, "{body}");
    assert_eq!(drawer[0]["number"], voucher.as_str());
    assert_eq!(drawer[0]["source"], "drawer");
    assert_eq!(drawer[0]["paidFrom"], "drawer");
    assert_eq!(drawer[0]["status"], "paid");
    assert_eq!(drawer[0]["movementType"], "petty_expense");

    let (_, body) = send(app, "GET", "/api/expenses", None, &manager).await;
    assert_eq!(body["data"]["total"], 3, "{body}");

    // Filters.
    let (_, body) = send(app, "GET", "/api/expenses?status=unpaid", None, &manager).await;
    assert_eq!(body["data"]["total"], 0, "the water bill is settled now");
    let (_, body) = send(app, "GET", "/api/expenses?search=water", None, &manager).await;
    assert_eq!(body["data"]["total"], 1);
    let (_, body) = send(app, "GET", "/api/expenses?source=ledger&from=2026-01-01", None, &manager).await;
    assert_eq!(body["data"]["total"], 1, "only the electricity bill is dated this year");
    let (status, _) = send(app, "GET", "/api/expenses?status=late", None, &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Figures: a new unpaid, overdue bill is owed; everything this month counts, drawer included.
    let (_, _) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": "2020-02-01", "categoryKey": electricity, "payeeName": "Landlord Co",
            "amountCents": 250_000, "paidFrom": "unpaid", "dueDate": "2020-02-10"
        })),
        &manager,
    )
    .await;
    let (status, body) = send(app, "GET", "/api/expenses/stats", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let stats = &body["data"];
    assert_eq!(stats["thisMonthCents"], 1_200_000 + 35_000);
    assert_eq!(stats["thisMonthDrawerCents"], 35_000);
    assert_eq!(stats["unpaidCents"], 250_000);
    assert_eq!(stats["unpaidCount"], 1);
    assert_eq!(stats["overdueCents"], 250_000);
    assert_eq!(stats["overdueCount"], 1);
}

#[tokio::test]
async fn the_expense_ledger_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn the_expense_ledger_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
