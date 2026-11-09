//! Resolving a credit note that was returned `pending_inspection`: the real condition decides
//! the stock and serial effects, the stored lines stay as they were, and voiding afterwards
//! reverses what the resolution restocked.

use axum::http::StatusCode;
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{
    admin_token, complete_simple_sale, get_product, seed_product, send_authed, staff_token,
};

const MANAGER_PIN: &str = "7391";

/// Sells one unit and returns it as `pending_inspection`. Returns the product id, the product
/// key and the credit note key.
async fn pending_credit_note(app: &common::TestApp, name: &str) -> (String, String, String) {
    let staff = staff_token(&app.config);
    let (product_id, product_key, price_cents) = seed_product(app, name, 2000, 10).await;
    let invoice_key = complete_simple_sale(app, &staff, &product_key, 1, price_cents, None).await;
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{
                "productKey": product_key,
                "quantity": 1,
                "condition": "pending_inspection"
            }],
            "paymentMethod": "cash"
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "awaiting_resolution");
    let key = body["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    (product_id, product_key, key)
}

async fn resolve(
    app: &common::TestApp,
    token: &str,
    key: &str,
    body: Value,
) -> (StatusCode, Value) {
    send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/credit-notes/{key}/resolve"),
        Some(body),
        token,
    )
    .await
}

/// A manager with `MANAGER_PIN`, so a staff approval has someone to name.
async fn manager_with_pin(app: &common::TestApp) -> String {
    let admin = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Manager Mia",
            "username": format!("mia_{}", &uuid::Uuid::new_v4().simple().to_string()[..6]),
            "password": "Password123!",
            "role": "manager",
        })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let claims = Claims {
        sub: body["data"]["id"]
            .as_str()
            .expect("the value is a string")
            .to_string(),
        exp: 9_999_999_999,
        role: Some(Role::Manager),
        permissions: roles::default_permissions(Role::Manager)
            .iter()
            .map(|p| p.to_string())
            .collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some("Manager Mia".to_string()),
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes");
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/auth/set-pin",
        Some(json!({ "pin": MANAGER_PIN })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    token
}

#[tokio::test]
async fn resolving_to_resalable_restocks_and_leaves_the_stored_line_alone() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (product_id, _, cn_key) = pending_credit_note(&app, "Resolvable Item").await;
    // Sold one of ten; the pending return put nothing back.
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 9);

    let (status, body) = resolve(
        &app,
        &admin,
        &cn_key,
        json!({ "lines": [{ "lineIndex": 0, "condition": "resalable" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let note = &body["data"];
    assert_eq!(note["status"], "resolved");
    assert_eq!(note["resolution"]["resolvedByName"], "Test User");
    assert_eq!(note["resolution"]["approvedByName"], "Test User");
    assert_eq!(note["resolution"]["lines"][0]["condition"], "resalable");
    assert_eq!(note["returnedItems"][0]["condition"], "pending_inspection");
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 10);

    // Once resolved it cannot be resolved again, and nothing is restocked twice.
    let (status, body) = resolve(
        &app,
        &admin,
        &cn_key,
        json!({ "lines": [{ "lineIndex": 0, "condition": "resalable" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "CREDIT_NOTE_NOT_AWAITING_RESOLUTION");
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 10);
}

#[tokio::test]
async fn resolving_to_damaged_needs_a_disposition_and_writes_nothing_back_to_stock() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (product_id, _, cn_key) = pending_credit_note(&app, "Scrap Item").await;

    for (lines, why) in [
        (json!([]), "no line decided"),
        (
            json!([{ "lineIndex": 1, "condition": "resalable" }]),
            "a line that is not pending",
        ),
        (
            json!([
                { "lineIndex": 0, "condition": "resalable" },
                { "lineIndex": 0, "condition": "resalable" }
            ]),
            "a line decided twice",
        ),
        (
            json!([{ "lineIndex": 0, "condition": "pending_inspection" }]),
            "still pending",
        ),
        (
            json!([{ "lineIndex": 0, "condition": "damaged" }]),
            "damaged without a disposition",
        ),
        (
            json!([{ "lineIndex": 0, "condition": "resalable", "disposition": "write_off_scrap" }]),
            "a disposition on a sellable unit",
        ),
    ] {
        let (status, body) = resolve(&app, &admin, &cn_key, json!({ "lines": lines })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {body}");
    }

    let (status, body) = resolve(
        &app,
        &admin,
        &cn_key,
        json!({ "lines": [{ "lineIndex": 0, "condition": "damaged", "disposition": "write_off_scrap" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["status"], "resolved");
    assert_eq!(
        body["data"]["resolution"]["lines"][0]["disposition"],
        "write_off_scrap"
    );
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 9);
}

#[tokio::test]
async fn an_admin_sends_no_approval_but_staff_need_a_manager_pin() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let staff = staff_token(&app.config);
    manager_with_pin(&app).await;
    let (product_id, _, cn_key) = pending_credit_note(&app, "Approval Item").await;
    let line = json!([{ "lineIndex": 0, "condition": "resalable" }]);

    let (status, body) = resolve(
        &app,
        &admin,
        &cn_key,
        json!({ "lines": line, "approval": { "managerPin": MANAGER_PIN, "reason": "checked" } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let (status, body) = resolve(&app, &staff, &cn_key, json!({ "lines": line })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PIN_NOT_SET");

    let (status, body) = resolve(
        &app,
        &staff,
        &cn_key,
        json!({ "lines": line, "approval": { "managerPin": "0000", "reason": "checked" } }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], "INVALID_PIN");
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 9);

    let (status, body) = resolve(
        &app,
        &staff,
        &cn_key,
        json!({ "lines": line, "approval": { "managerPin": MANAGER_PIN, "reason": "Unit tested fine" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["resolution"]["resolvedByName"], "Test User");
    assert_eq!(body["data"]["resolution"]["approvedByName"], "Manager Mia");
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 10);
}

#[tokio::test]
async fn voiding_a_resolved_credit_note_takes_the_restocked_unit_out_again() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (product_id, _, cn_key) = pending_credit_note(&app, "Void After Resolve").await;

    let (status, body) = resolve(
        &app,
        &admin,
        &cn_key,
        json!({ "lines": [{ "lineIndex": 0, "condition": "resalable" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 10);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        &format!("/api/billing/credit-notes/{cn_key}/void"),
        Some(json!({ "reason": "Entered in error" })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(get_product(&app, &product_id).await["stockQuantity"], 9);

    // A voided note cannot be resolved.
    let (status, body) = resolve(
        &app,
        &admin,
        &cn_key,
        json!({ "lines": [{ "lineIndex": 0, "condition": "resalable" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}
