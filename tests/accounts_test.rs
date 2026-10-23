// The money accounts: the drawer's cash, and the bank, card and online balances worked out from an
// opening balance plus sales, income, expenses and cash moved to and from the bank. On SQLite and
// on a throwaway Atlas DB.

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

fn account<'a>(body: &'a Value, kind: &str) -> &'a Value {
    body["data"]["accounts"].as_array().unwrap().iter().find(|a| a["kind"] == kind).unwrap()
}

async fn scenario(app: &common::TestApp) {
    let staff = common::mint_token(&app.config, Some(Role::Staff), roles::default_permissions(Role::Staff));
    let manager = common::mint_token(&app.config, Some(Role::Manager), roles::default_permissions(Role::Manager));
    let admin = common::mint_token(&app.config, Some(Role::Admin), roles::default_permissions(Role::Admin));

    // A cashier cannot see the shop's accounts; a manager can look but not set where they start.
    let (status, _) = send(app, "GET", "/api/accounts", None, &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let opening = |cents: i64| json!({ "openingBalanceCents": cents, "asOfDate": today() });
    let (status, _) = send(app, "PUT", "/api/accounts/bank/opening", Some(opening(1)), &manager).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Before any opening balance only the drawer has a balance.
    let (status, body) = send(app, "GET", "/api/accounts", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(account(&body, "drawer")["balanceCents"], 0);
    for kind in ["bank", "card", "online"] {
        let a = account(&body, kind);
        assert!(a.get("balanceCents").is_none() && a.get("openingBalanceCents").is_none(), "{kind}: {a}");
    }

    // What cannot be set.
    for (what, uri, body) in [
        ("the drawer has no opening", "/api/accounts/drawer/opening", opening(1)),
        ("an unknown account", "/api/accounts/cheque/opening", opening(1)),
        ("a made-up date", "/api/accounts/bank/opening", json!({ "openingBalanceCents": 1, "asOfDate": "2026-02-30" })),
        ("a future date", "/api/accounts/bank/opening", json!({ "openingBalanceCents": 1, "asOfDate": "2999-01-01" })),
    ] {
        let (status, body) = send(app, "PUT", uri, Some(body), &admin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
    }

    // The owner sets where the bank and card accounts start.
    let (status, body) = send(app, "PUT", "/api/accounts/bank/opening", Some(opening(1_000_000)), &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["balanceCents"], 1_000_000);
    assert_eq!(body["data"]["activity"]["expensesCents"], 0);
    let (status, _) = send(app, "PUT", "/api/accounts/card/opening", Some(opening(0)), &admin).await;
    assert_eq!(status, StatusCode::OK);

    // Bank: an expense and an income, a deposit from the drawer and a withdrawal back to it.
    let (status, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": today(), "categoryKey": cat(CashMovementType::RentAndBills, "Electricity"),
            "amountCents": 120_000, "paidFrom": "bank"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = send(
        app,
        "POST",
        "/api/incomes",
        Some(json!({
            "incomeDate": today(), "categoryKey": cat(CashMovementType::AddCash, "Commission received"),
            "amountCents": 450_000, "receivedInto": "bank"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let movement = |t: &str, amount: i64, category: String| {
        json!({ "movementType": t, "amountCents": amount, "terminalId": "term_acc", "categoryKey": category })
    };
    for (t, amount, category) in [
        ("add_cash", 5_000_000, cat(CashMovementType::AddCash, "Opening float")),
        ("cash_drop", 200_000, cat(CashMovementType::CashDrop, "To bank deposit")),
        ("cash_drop", 70_000, cat(CashMovementType::CashDrop, "To safe")),
        ("add_cash", 50_000, cat(CashMovementType::AddCash, "Bank withdrawal to drawer")),
    ] {
        let (status, body) = send(app, "POST", "/api/sessions/movements", Some(movement(t, amount, category)), &staff).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    // Card: a sale paid by card, and a cost paid by card.
    let (status, body) = send(
        app,
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Nimal" },
            "items": [{ "name": "Repair", "unitPriceCents": 150_000, "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "card", "isCredit": false, "cardLast4": "4242" },
            "shopProfileSnapshot": { "tradingName": "TechFix" }
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": today(), "categoryKey": cat(CashMovementType::RentAndBills, "Software subscriptions"),
            "amountCents": 20_000, "paidFrom": "card"
        })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = send(app, "GET", "/api/accounts", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let bank = account(&body, "bank");
    assert_eq!(bank["activity"]["incomeCents"], 450_000);
    assert_eq!(bank["activity"]["expensesCents"], 120_000);
    assert_eq!(bank["activity"]["depositsCents"], 200_000, "only the bank deposit, not the safe");
    assert_eq!(bank["activity"]["withdrawalsCents"], 50_000);
    assert_eq!(bank["balanceCents"], 1_000_000 + 450_000 + 200_000 - 120_000 - 50_000);
    let card = account(&body, "card");
    assert_eq!(card["activity"]["salesCents"], 150_000);
    assert_eq!(card["balanceCents"], 150_000 - 20_000);
    assert!(account(&body, "online").get("balanceCents").is_none(), "online was never set up");
    // 5,000,000 float, less both drops, plus the withdrawal.
    assert_eq!(account(&body, "drawer")["balanceCents"], 5_000_000 - 200_000 - 70_000 + 50_000);

    // Changing the opening balance moves the balance.
    let (status, body) = send(app, "PUT", "/api/accounts/bank/opening", Some(opening(0)), &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["balanceCents"], 450_000 + 200_000 - 120_000 - 50_000);
}

#[tokio::test]
async fn the_accounts_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn the_accounts_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
