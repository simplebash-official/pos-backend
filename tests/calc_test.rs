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
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
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
