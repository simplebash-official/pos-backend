// The drawer rules: seeded starting values, who may read and change them, validation, and that the
// undo window and the shop-wide approval limit really drive the cash movement rules. On SQLite and
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

async fn scenario(app: &common::TestApp) {
    let staff = common::mint_token(&app.config, Some(Role::Staff), roles::default_permissions(Role::Staff));
    let manager = common::mint_token(&app.config, Some(Role::Manager), roles::default_permissions(Role::Manager));
    let admin = common::mint_token(&app.config, Some(Role::Admin), roles::default_permissions(Role::Admin));

    // Every shop starts with the shipped values; the cashier reads them for the cash form.
    let (status, body) = send(app, "GET", "/api/cash-settings", None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["undoWindowSeconds"], 120);
    assert_eq!(body["data"]["quickAmountsCents"], json!([10_000, 50_000, 100_000, 500_000]));
    assert!(body["data"].get("approvalThresholdCents").is_none() && body["data"].get("floatTargetCents").is_none());

    // Only the owner changes them.
    let rules = |undo: i64| json!({ "undoWindowSeconds": undo, "approvalThresholdCents": 20_000, "quickAmountsCents": [5_000, 25_000], "floatTargetCents": 300_000 });
    for token in [&staff, &manager] {
        let (status, _) = send(app, "PUT", "/api/cash-settings", Some(rules(60)), token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    for (what, body) in [
        ("undo window too short", rules(5)),
        ("undo window too long", rules(7200)),
        ("no quick amounts", json!({ "undoWindowSeconds": 60, "quickAmountsCents": [] })),
        ("quick amounts out of order", json!({ "undoWindowSeconds": 60, "quickAmountsCents": [50_000, 10_000] })),
        ("a zero approval limit", json!({ "undoWindowSeconds": 60, "approvalThresholdCents": 0, "quickAmountsCents": [1_000] })),
        ("a negative float target", json!({ "undoWindowSeconds": 60, "floatTargetCents": -5, "quickAmountsCents": [1_000] })),
    ] {
        let (status, body) = send(app, "PUT", "/api/cash-settings", Some(body), &admin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {body}");
    }
    let (status, body) = send(app, "PUT", "/api/cash-settings", Some(rules(30)), &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["undoWindowSeconds"], 30);
    assert_eq!(body["data"]["approvalThresholdCents"], 20_000);
    assert_eq!(body["data"]["floatTargetCents"], 300_000);
    assert_eq!(body["data"]["version"], 2);

    // The undo window drives `undoUntil`.
    let petty = |amount: i64| json!({
        "movementType": "petty_expense", "amountCents": amount, "terminalId": "term_set",
        "categoryKey": category_key(CashMovementType::PettyExpense, "Fuel")
    });
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "add_cash", "amountCents": 5_000_000, "terminalId": "term_set",
            "categoryKey": category_key(CashMovementType::AddCash, "Opening float")
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(petty(10_000)), &staff).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let created = chrono::DateTime::parse_from_rfc3339(body["data"]["createdAt"].as_str().unwrap()).unwrap();
    let until = chrono::DateTime::parse_from_rfc3339(body["data"]["undoUntil"].as_str().unwrap()).unwrap();
    assert_eq!((until - created).num_seconds(), 30);

    // The shop-wide limit asks for a manager PIN above it, whatever the category.
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(petty(25_000)), &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PIN_NOT_SET");

    // Clearing the limit lifts it.
    let (status, _) = send(
        app,
        "PUT",
        "/api/cash-settings",
        Some(json!({ "undoWindowSeconds": 30, "quickAmountsCents": [5_000, 25_000] })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(petty(25_000)), &staff).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, body) = send(app, "GET", "/api/cash-settings", None, &staff).await;
    assert!(body["data"].get("approvalThresholdCents").is_none() && body["data"].get("floatTargetCents").is_none());
}

#[tokio::test]
async fn cash_settings_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn cash_settings_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
