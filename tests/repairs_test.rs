mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jana2u_pos_backend::{core::constants::roles, domain::users::Role};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(request).await.unwrap();
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

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    execute(router, request).await
}

async fn send_authed(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(match body {
            Some(value) => Body::from(value.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    execute(router, request).await
}

fn staff_token(config: &jana2u_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

fn unprivileged_token(config: &jana2u_pos_backend::core::config::Config) -> String {
    common::mint_token(config, Some(Role::Staff), &[])
}

fn sample_repair_payload(phone_suffix: &str) -> Value {
    json!({
        "customerName": "Kasun Silva",
        "customerPhone": format!("077{phone_suffix}"),
        "deviceModel": "iPhone 14",
        "issueDescription": "Cracked screen",
        "estimatedCostCents": 850000,
    })
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let app = common::spawn_app().await;

    let (status, _) = send(&app.router, "GET", "/api/repairs", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(
        &app.router,
        "POST",
        "/api/repairs",
        Some(sample_repair_payload("1234567")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_without_repairs_write_permission_is_rejected() {
    let app = common::spawn_app().await;
    let token = unprivileged_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(sample_repair_payload("1111111")),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Body: {body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn create_repair_reserves_ticket_number_and_defaults_status_to_received() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(sample_repair_payload("2222222")),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let data = &body["data"];
    assert_eq!(data["status"], "received");
    assert_eq!(data["customerName"], "Kasun Silva");
    assert_eq!(data["estimatedCostCents"], 850000);
    assert!(
        data["ticketNumber"].as_str().unwrap().starts_with("REP-"),
        "ticket number must start with REP-, got {}",
        data["ticketNumber"]
    );
    assert!(data["key"].as_str().unwrap().starts_with("rep_"));
}

#[tokio::test]
async fn create_repair_rejects_invalid_status_and_negative_cost() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let mut payload = sample_repair_payload("3333333");
    payload["status"] = json!("not-a-real-status");
    let (status, body) =
        send_authed(&app.router, "POST", "/api/repairs", Some(payload), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
    assert_eq!(body["code"], "INVALID_JOB_STATUS");

    let mut payload = sample_repair_payload("4444444");
    payload["estimatedCostCents"] = json!(-100);
    let (status, body) =
        send_authed(&app.router, "POST", "/api/repairs", Some(payload), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "Body: {body}");
}

#[tokio::test]
async fn get_repair_by_id_and_key_and_update_status() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (_, created) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(sample_repair_payload("5555555")),
        &token,
    )
    .await;
    let id = created["data"]["id"].as_str().unwrap();
    let key = created["data"]["key"].as_str().unwrap();

    let (status, by_id) = send_authed(
        &app.router,
        "GET",
        &format!("/api/repairs/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_id["data"]["key"], key);

    let (status, by_key) = send_authed(
        &app.router,
        "GET",
        &format!("/api/repairs/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_key["data"]["id"], id);

    let (status, updated) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/repairs/{key}"),
        Some(json!({ "status": "in_repair" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {updated}");
    assert_eq!(updated["data"]["status"], "in_repair");
    assert_eq!(
        updated["data"]["customerName"], "Kasun Silva",
        "unspecified fields must be left unchanged"
    );
}

#[tokio::test]
async fn list_repairs_filters_by_search_and_status() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let unique = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let mut payload = sample_repair_payload("6666666");
    payload["customerName"] = json!(format!("Findable-{unique}"));
    send_authed(&app.router, "POST", "/api/repairs", Some(payload), &token).await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/repairs?search=Findable-{unique}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let repairs = body["data"]["repairs"].as_array().unwrap();
    assert_eq!(repairs.len(), 1, "Body: {body}");
    assert_eq!(repairs[0]["customerName"], format!("Findable-{unique}"));

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/repairs?status=cancelled",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for repair in body["data"]["repairs"].as_array().unwrap() {
        assert_eq!(repair["status"], "cancelled");
    }
}

#[tokio::test]
async fn delete_repair_soft_deletes_and_excludes_from_lists() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (_, created) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(sample_repair_payload("7777777")),
        &token,
    )
    .await;
    let key = created["data"]["key"].as_str().unwrap();

    let (status, _) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/repairs/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/repairs/{key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Body: {body}");
    assert_eq!(body["code"], "REPAIR_NOT_FOUND");
}

async fn seed_customer(app: &common::TestApp, phone_suffix: &str) -> (String, String) {
    let token = staff_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/customers",
        Some(json!({
            "name": "Real Customer Name",
            "primaryPhone": format!("071{phone_suffix}"),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Body: {body}");
    (
        body["data"]["key"].as_str().unwrap().to_string(),
        body["data"]["primaryPhone"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn create_repair_with_customer_key_resolves_name_and_phone_from_customer_record() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (customer_key, customer_phone) = seed_customer(&app, "8888881").await;

    // customerName/customerPhone here are deliberately wrong — a valid
    // customerKey must override both from the linked customer record.
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(json!({
            "customerKey": customer_key,
            "customerName": "Wrong Name",
            "customerPhone": "0009999999",
            "deviceModel": "iPhone 14",
            "issueDescription": "Cracked screen",
            "estimatedCostCents": 850000,
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Body: {body}");
    let data = &body["data"];
    assert_eq!(data["customerName"], "Real Customer Name");
    assert_eq!(data["customerPhone"], customer_phone);
}

#[tokio::test]
async fn update_repair_with_customer_key_resolves_name_and_phone_from_customer_record() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);
    let (_, created) = send_authed(
        &app.router,
        "POST",
        "/api/repairs",
        Some(sample_repair_payload("8888882")),
        &token,
    )
    .await;
    let key = created["data"]["key"].as_str().unwrap();

    let (customer_key, customer_phone) = seed_customer(&app, "8888883").await;

    let (status, updated) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/repairs/{key}"),
        Some(json!({
            "customerKey": customer_key,
            "customerName": "Wrong Name",
            "customerPhone": "0009999999",
        })),
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Body: {updated}");
    assert_eq!(updated["data"]["customerName"], "Real Customer Name");
    assert_eq!(updated["data"]["customerPhone"], customer_phone);
}
