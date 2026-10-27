// Receipt photos: attaching an image to a cash movement, expense or income, who may see and add
// one, and refusing anything that is not a small real image. On SQLite and on a throwaway Atlas DB.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::constants::roles,
    domain::{cash_categories::category_key, sessions::CashMovementType, users::Role},
};
use tower::ServiceExt;

async fn raw(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Option<String>, Vec<u8>) {
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
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, content_type, bytes)
}

async fn send(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let (status, _, bytes) = raw(app, method, uri, body, token).await;
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

fn today() -> String {
    chrono::Utc::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// The first bytes of a real JPEG followed by filler; enough for the type check.
fn jpeg() -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F'];
    bytes.extend(std::iter::repeat_n(7u8, 200));
    bytes
}

fn upload(kind: &str, key: &str, bytes: &[u8], content_type: &str) -> Value {
    json!({
        "ownerKind": kind, "ownerKey": key, "fileName": "receipt.jpg",
        "contentType": content_type, "dataBase64": STANDARD.encode(bytes)
    })
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

    // A drawer movement by the cashier, a bank expense and an income by the manager.
    for amount in [5_000_000] {
        let (status, body) = send(
            app,
            "POST",
            "/api/sessions/movements",
            Some(json!({
                "movementType": "add_cash", "amountCents": amount, "terminalId": "term_att",
                "categoryKey": category_key(CashMovementType::AddCash, "Opening float")
            })),
            &staff,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let (status, body) = send(
        app,
        "POST",
        "/api/sessions/movements",
        Some(json!({
            "movementType": "petty_expense", "amountCents": 35_000, "terminalId": "term_att",
            "categoryKey": category_key(CashMovementType::PettyExpense, "Fuel")
        })),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let movement_key = body["data"]["key"].as_str().unwrap().to_string();
    let (_, body) = send(
        app,
        "POST",
        "/api/expenses",
        Some(json!({
            "expenseDate": today(), "categoryKey": category_key(CashMovementType::RentAndBills, "Electricity"),
            "amountCents": 100_000, "paidFrom": "bank"
        })),
        &manager,
    )
    .await;
    let expense_key = body["data"]["expense"]["key"].as_str().unwrap().to_string();

    // The cashier attaches the receipt to their own movement, and sees it.
    let (status, body) = send(
        app,
        "POST",
        "/api/attachments",
        Some(upload(
            "cash_movement",
            &movement_key,
            &jpeg(),
            "image/jpeg",
        )),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let key = body["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["contentType"], "image/jpeg");
    assert_eq!(body["data"]["sizeBytes"], jpeg().len());
    assert!(
        body["data"].get("dataBase64").is_none(),
        "the list never carries the image"
    );
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/attachments?ownerKind=cash_movement&ownerKey={movement_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // The image comes back byte for byte with its type.
    let (status, content_type, bytes) = raw(
        app,
        "GET",
        &format!("/api/attachments/{key}/content"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("image/jpeg"));
    assert_eq!(bytes, jpeg());
    let (status, _) = send(
        app,
        "GET",
        "/api/attachments/att_nope/content",
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Who may touch an expense's photos follows the expense: a cashier may not.
    let (status, _) = send(
        app,
        "POST",
        "/api/attachments",
        Some(upload("expense", &expense_key, &jpeg(), "image/jpeg")),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        app,
        "GET",
        &format!("/api/attachments?ownerKind=expense&ownerKey={expense_key}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(
        app,
        "POST",
        "/api/attachments",
        Some(upload("expense", &expense_key, &jpeg(), "image/jpeg")),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // What cannot be attached.
    let png = [
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A][..],
        &[0u8; 50],
    ]
    .concat();
    let big = [&jpeg()[..], &vec![0u8; 1_048_576][..]].concat();
    for (what, body, expected) in [
        (
            "a PDF",
            upload("expense", &expense_key, b"%PDF-1.4 ...", "application/pdf"),
            StatusCode::BAD_REQUEST,
        ),
        (
            "a PNG called a JPEG",
            upload("expense", &expense_key, &png, "image/jpeg"),
            StatusCode::BAD_REQUEST,
        ),
        (
            "something too big",
            upload("expense", &expense_key, &big, "image/jpeg"),
            StatusCode::BAD_REQUEST,
        ),
        (
            "not base64",
            json!({ "ownerKind": "expense", "ownerKey": expense_key, "fileName": "a.jpg", "contentType": "image/jpeg", "dataBase64": "***" }),
            StatusCode::BAD_REQUEST,
        ),
        (
            "no name",
            json!({ "ownerKind": "expense", "ownerKey": expense_key, "fileName": " ", "contentType": "image/jpeg", "dataBase64": STANDARD.encode(jpeg()) }),
            StatusCode::BAD_REQUEST,
        ),
        (
            "a record that does not exist",
            upload("expense", "exp_nope", &jpeg(), "image/jpeg"),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (status, body) = send(app, "POST", "/api/attachments", Some(body), &manager).await;
        assert_eq!(status, expected, "{what}: {body}");
    }

    // A record holds at most five photos.
    for _ in 0..3 {
        let (status, body) = send(
            app,
            "POST",
            "/api/attachments",
            Some(upload("expense", &expense_key, &jpeg(), "image/jpeg")),
            &manager,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let (_, body) = send(
        app,
        "GET",
        &format!("/api/attachments?ownerKind=expense&ownerKey={expense_key}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(body["data"].as_array().unwrap().len(), 4);
    let (status, _) = send(
        app,
        "POST",
        "/api/attachments",
        Some(upload("expense", &expense_key, &jpeg(), "image/jpeg")),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body) = send(
        app,
        "POST",
        "/api/attachments",
        Some(upload("expense", &expense_key, &jpeg(), "image/jpeg")),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "ATTACHMENT_LIMIT");
}

#[tokio::test]
async fn receipt_photos_on_sqlite() {
    let app = common::spawn_app_sqlite().await;
    scenario(&app).await;
}

#[tokio::test]
async fn receipt_photos_on_mongo() {
    let app = common::spawn_app_isolated().await;
    scenario(&app).await;
    app.db.drop().await.ok();
}
