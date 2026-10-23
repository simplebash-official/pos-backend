// Drawer safety: a payout bigger than the cash in the drawer needs a manager's PIN and a reason, and
// any movement can be undone by recording its opposite (never by editing or deleting it). On SQLite
// and on a throwaway Atlas database.

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
    core::{constants::roles, middleware::auth::Claims},
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

fn token_for(app: &common::TestApp, user: &str, role: Role, name: &str) -> String {
    let claims = Claims {
        sub: user.to_string(),
        exp: 9_999_999_999,
        role: Some(role),
        permissions: roles::default_permissions(role).iter().map(|p| p.to_string()).collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some(name.to_string()),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
    )
    .unwrap()
}

fn cat(t: CashMovementType, name: &str) -> String {
    category_key(t, name)
}

async fn scenario(app: &common::TestApp) {
    simplebash_pos_backend::seeds::admin::seed_admin(&app.db_handle, None, None).await.unwrap();
    let admin = token_for(app, "admin", Role::Admin, "Admin");

    let make = |name: &'static str, role: &'static str| {
        let admin = admin.clone();
        async move {
            let (status, body) = send(
                app,
                "POST",
                "/api/users",
                Some(json!({
                    "name": name, "username": format!("{name}_{}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
                    "password": "Password123!", "role": role
                })),
                &admin,
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "{body}");
            body["data"]["id"].as_str().unwrap().to_string()
        }
    };
    let manager_id = make("mgr", "manager").await;
    let alice_id = make("alice", "staff").await;
    let bob_id = make("bob", "staff").await;
    let manager = token_for(app, &manager_id, Role::Manager, "Manager Mia");
    let alice = token_for(app, &alice_id, Role::Staff, "Alice");
    let bob = token_for(app, &bob_id, Role::Staff, "Bob");

    let (status, _) = send(app, "POST", "/api/auth/set-pin", Some(json!({ "pin": "5555" })), &manager).await;
    assert_eq!(status, StatusCode::OK);

    let terminal = format!("term_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/open",
        Some(json!({ "terminalId": terminal, "openingFloatCents": 10_000 })),
        &alice,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let session = body["data"]["key"].as_str().unwrap().to_string();

    let tea = cat(CashMovementType::PettyExpense, "Tea and refreshments");
    let expense = |amount: i64, extra: Value| {
        let mut base = json!({
            "movementType": "petty_expense", "amountCents": amount, "sessionKey": session,
            "terminalId": terminal, "categoryKey": tea
        });
        base.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        base
    };
    let expected = |app| {
        let session = session.clone();
        let alice = alice.clone();
        async move {
            let (_, body) = send(app, "GET", &format!("/api/sessions/{session}"), None, &alice).await;
            body["data"]["expectedCashCents"].as_i64().unwrap()
        }
    };
    assert_eq!(expected(app).await, 10_000);

    // More than the drawer holds: refused, whatever the cashier says.
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(expense(20_000, json!({}))), &alice).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "DRAWER_CASH_EXCEEDED");
    assert!(body["message"].as_str().unwrap().contains("Rs. 100.00"), "{body}");
    // A PIN alone is not enough: the manager must also say why.
    let (status, body) = send(
        app, "POST", "/api/sessions/movements", Some(expense(20_000, json!({ "managerPin": "5555" }))), &alice,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, _) = send(
        app, "POST", "/api/sessions/movements",
        Some(expense(20_000, json!({ "managerPin": "0000", "overrideReason": "Supplier waiting" }))), &alice,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // A reason without needing one is refused rather than silently kept.
    let (status, _) = send(
        app, "POST", "/api/sessions/movements", Some(expense(1_000, json!({ "overrideReason": "Just because" }))), &alice,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(expected(app).await, 10_000, "nothing was recorded by the refusals");

    // A manager's PIN and a reason let it through, and both are kept on the entry.
    let (status, body) = send(
        app, "POST", "/api/sessions/movements",
        Some(expense(20_000, json!({ "managerPin": "5555", "overrideReason": "Supplier waiting at the door" }))), &alice,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["approvedByNameSnapshot"], "mgr");
    assert_eq!(body["data"]["approvalReason"], "Supplier waiting at the door");
    let over = body["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(expected(app).await, -10_000);

    // A drop is a payout too.
    let (status, body) = send(
        app, "POST", "/api/sessions/movements",
        Some(json!({ "movementType": "cash_drop", "amountCents": 100, "sessionKey": session, "terminalId": terminal,
                     "categoryKey": cat(CashMovementType::CashDrop, "To safe") })), &alice,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "the drawer is already short: {body}");

    // Undo: the recorder can undo inside the window; the original is untouched and gains a reversal.
    let (status, body) = send(app, "POST", &format!("/api/sessions/movements/{over}/reverse"), Some(json!({})), &alice).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let reversal = &body["data"];
    assert_eq!(reversal["reversesKey"], over.as_str());
    assert_eq!(reversal["direction"], "in");
    assert_eq!(reversal["amountCents"], 20_000);
    assert!(reversal["note"].as_str().unwrap().starts_with("Reversal of CSH-"));
    assert!(reversal.get("undoUntil").is_none(), "a reversal cannot be undone again");
    assert_eq!(expected(app).await, 10_000, "the drawer is back where it was");

    let (_, body) = send(app, "GET", &format!("/api/sessions/movements/{over}"), None, &manager).await;
    assert_eq!(body["data"]["reversedByKey"], reversal["key"]);
    assert!(body["data"].get("undoUntil").is_none());
    assert_eq!(body["data"]["amountCents"], 20_000, "the original row is never changed");

    // Once, and never a reversal of a reversal.
    let (status, body) = send(app, "POST", &format!("/api/sessions/movements/{over}/reverse"), Some(json!({})), &alice).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "MOVEMENT_ALREADY_REVERSED");
    let rev_key = reversal["key"].as_str().unwrap();
    let (status, _) = send(app, "POST", &format!("/api/sessions/movements/{rev_key}/reverse"), Some(json!({})), &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Someone else's movement: a cashier cannot, a manager can with a reason.
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(expense(3_000, json!({}))), &alice).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let small = body["data"]["key"].as_str().unwrap().to_string();
    assert!(body["data"]["undoUntil"].is_string(), "a fresh movement can be undone for a while");
    assert_eq!(expected(app).await, 7_000);
    let (status, _) = send(app, "POST", &format!("/api/sessions/movements/{small}/reverse"), Some(json!({})), &bob).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(app, "POST", &format!("/api/sessions/movements/{small}/reverse"), Some(json!({})), &manager).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a manager must say why");
    let (status, body) = send(
        app, "POST", &format!("/api/sessions/movements/{small}/reverse"), Some(json!({ "reason": "Wrong amount" })), &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["approvalReason"], "Wrong amount");
    assert_eq!(expected(app).await, 10_000);

    // A no-sale open has nothing to reverse, and an unknown key is not found.
    let (_, body) = send(
        app, "POST", "/api/sessions/movements",
        Some(json!({ "movementType": "no_sale", "amountCents": 0, "sessionKey": session, "terminalId": terminal,
                     "categoryKey": cat(CashMovementType::NoSale, "Give change") })), &alice,
    )
    .await;
    assert!(body["data"].get("undoUntil").is_none());
    let no_sale = body["data"]["key"].as_str().unwrap().to_string();
    let (status, _) = send(app, "POST", &format!("/api/sessions/movements/{no_sale}/reverse"), Some(json!({})), &alice).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(app, "POST", "/api/sessions/movements/cshm_nope/reverse", Some(json!({})), &alice).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A reversed drawer payment stops being a cost on the Expenses page.
    let (status, body) = send(app, "GET", "/api/expenses?source=drawer", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["total"], 0, "both payments were reversed: {body}");
    let (status, body) = send(app, "POST", "/api/sessions/movements", Some(expense(1_500, json!({}))), &alice).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (_, body) = send(app, "GET", "/api/expenses?source=drawer", None, &manager).await;
    assert_eq!(body["data"]["total"], 1, "a payment that was not reversed still counts: {body}");

    // The shift's own totals are net of the reversals.
    let (_, body) = send(app, "GET", &format!("/api/sessions/{session}"), None, &alice).await;
    assert_eq!(body["data"]["totalPayOutCents"], 1_500);
}

#[tokio::test]
async fn drawer_safety_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn drawer_safety_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
