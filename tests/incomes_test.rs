// The income ledger: money received into the bank, a card or online, and cash added to the drawer
// under an income category appearing in the same list. On SQLite and on a throwaway Atlas DB.

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

fn add_cash(name: &str) -> String {
    category_key(CashMovementType::AddCash, name)
}

fn today() -> String {
    chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()
}

async fn scenario(app: &common::TestApp) {
    let staff = common::mint_token(&app.config, Some(Role::Staff), roles::default_permissions(Role::Staff));
    let manager = common::mint_token(&app.config, Some(Role::Manager), roles::default_permissions(Role::Manager));
    let scrap = add_cash("Scrap and waste sale");

    // A cashier sees the drawer's movements, not the shop's income.
    for (method, uri) in [("GET", "/api/incomes"), ("GET", "/api/incomes/stats")] {
        let (status, _) = send(app, method, uri, None, &staff).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    let (status, _) = send(
        app,
        "POST",
        "/api/incomes",
        Some(json!({ "incomeDate": today(), "categoryKey": scrap, "amountCents": 100, "receivedInto": "bank" })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Income categories are the add-cash ones that count as income, nothing else.
    let (status, body) = send(app, "GET", "/api/cash-categories?forIncome=true", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let names: Vec<&str> = body["data"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Scrap and waste sale") && names.contains(&"Other income"), "{names:?}");
    assert!(!names.contains(&"Owner capital") && !names.contains(&"Fuel"), "{names:?}");

    // Money received into the bank is entered once.
    let (status, body) = send(
        app,
        "POST",
        "/api/incomes",
        Some(json!({
            "incomeDate": today(), "categoryKey": add_cash("Commission received"), "payeeName": "Dialog",
            "amountCents": 450_000, "receivedInto": "bank", "referenceNo": "TXN-9", "note": "October"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let income = &body["data"];
    assert!(income["number"].as_str().unwrap().starts_with("INC-"));
    assert_eq!(income["source"], "ledger");
    assert_eq!(income["receivedInto"], "bank");
    assert_eq!(income["categoryName"], "Commission received");
    assert_eq!(income["reportGroup"], "other_income");
    assert!(income["payeeKey"].as_str().unwrap().starts_with("pye_other_"), "the typed name is saved for next time");
    let key = income["key"].as_str().unwrap().to_string();
    let (status, body) = send(app, "GET", &format!("/api/incomes/{key}"), None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["amountCents"], 450_000);
    let (status, _) = send(app, "GET", "/api/incomes/inc_nope", None, &manager).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // What cannot be entered.
    let good = || json!({ "incomeDate": today(), "categoryKey": scrap, "amountCents": 100, "receivedInto": "online" });
    for (what, extra) in [
        ("drawer cash is a cash movement", json!({ "receivedInto": "drawer" })),
        ("no amount", json!({ "amountCents": 0 })),
        ("a made-up date", json!({ "incomeDate": "2026-02-30" })),
        ("owner capital is not income", json!({ "categoryKey": add_cash("Owner capital") })),
        ("a loan received is not income", json!({ "categoryKey": add_cash("Loan received") })),
        ("a cost category is not income", json!({ "categoryKey": category_key(CashMovementType::PettyExpense, "Fuel") })),
    ] {
        let mut body = good();
        body.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let (status, body) = send(app, "POST", "/api/incomes", Some(body), &manager).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
    }

    // Cash added to the drawer under an income category is read into the same list; a float is not.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash", "amountCents": 5_000_000, "terminalId": "term_inc",
            "categoryKey": add_cash("Opening float")
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
            "movementType": "add_cash", "amountCents": 80_000, "terminalId": "term_inc",
            "categoryKey": scrap, "payeeName": "Recycler"
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let voucher = body["data"]["movementNumber"].as_str().unwrap().to_string();
    let movement_key = body["data"]["key"].as_str().unwrap().to_string();
    // Reversed later: both rows leave the list.
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash", "amountCents": 30_000, "terminalId": "term_inc",
            "categoryKey": add_cash("Interest received")
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let mistaken = body["data"]["key"].as_str().unwrap().to_string();
    let (status, body) = send(
        app,
        "POST",
        &format!("/api/sessions/movements/{mistaken}/reverse"),
        Some(json!({})),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = send(app, "GET", "/api/incomes?source=drawer", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let drawer = body["data"]["incomes"].as_array().unwrap();
    assert_eq!(drawer.len(), 1, "{body}");
    assert_eq!(drawer[0]["key"], movement_key.as_str());
    assert_eq!(drawer[0]["number"], voucher.as_str());
    assert_eq!(drawer[0]["receivedInto"], "drawer");
    assert_eq!(drawer[0]["payeeName"], "Recycler");

    let (_, body) = send(app, "GET", "/api/incomes", None, &manager).await;
    assert_eq!(body["data"]["total"], 2, "{body}");

    // Filters.
    let (_, body) = send(app, "GET", "/api/incomes?search=dialog", None, &manager).await;
    assert_eq!(body["data"]["total"], 1);
    let (_, body) = send(app, "GET", &format!("/api/incomes?categoryKey={scrap}"), None, &manager).await;
    assert_eq!(body["data"]["total"], 1);
    let (_, body) = send(app, "GET", "/api/incomes?source=ledger&from=2020-01-01", None, &manager).await;
    assert_eq!(body["data"]["total"], 1);
    let (status, _) = send(app, "GET", "/api/incomes?source=bank", None, &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Figures.
    let (status, body) = send(app, "GET", "/api/incomes/stats", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["thisMonthCents"], 450_000 + 80_000);
    assert_eq!(body["data"]["thisMonthDrawerCents"], 80_000);
    assert_eq!(body["data"]["lastMonthCents"], 0);
}

#[tokio::test]
async fn the_income_ledger_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn the_income_ledger_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
