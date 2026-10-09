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
use uuid::Uuid;

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

fn admin_token(config: &simplebash_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

fn manager_token(config: &simplebash_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    )
}

/// Staff's default permission set has `employees:read` but not
/// `employees:write` — every write route should 403 this token, every read
/// route should succeed.
fn staff_token(config: &simplebash_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

fn sample_employee_payload() -> Value {
    json!({
        "name": format!("Test Technician {}", Uuid::new_v4()),
        "phone": "0771234567",
        "role": "technician",
        "defaultSplitType": "percentage",
        "defaultSplitValue": 20.0,
    })
}

async fn create_employee_via_api(router: &axum::Router, token: &str) -> (String, Value) {
    let (status, body) = send_authed(
        router,
        "POST",
        "/api/employees",
        Some(sample_employee_payload()),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let id = body["data"]["id"].as_str().unwrap().to_string();
    (id, body["data"].clone())
}

// === Create ===

#[tokio::test]
async fn admin_can_create_employee() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (_, employee) = create_employee_via_api(&app.router, &token).await;
    assert_eq!(employee["role"], "technician");
    assert_eq!(employee["defaultSplitType"], "percentage");
    assert!(employee["login"].is_null());
}

#[tokio::test]
async fn manager_can_create_employee() {
    let app = common::spawn_app().await;
    let token = manager_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/employees",
        Some(sample_employee_payload()),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
}

#[tokio::test]
async fn staff_cannot_create_employee() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/employees",
        Some(sample_employee_payload()),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body:?}");
}

#[tokio::test]
async fn creating_employee_without_token_is_unauthorized() {
    let app = common::spawn_app().await;
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/employees",
        Some(sample_employee_payload()),
        "not-a-real-token",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// === Read ===

#[tokio::test]
async fn staff_can_list_and_get_employees() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let staff = staff_token(&app.config);

    let (id, _) = create_employee_via_api(&app.router, &admin).await;

    let (status, body) = send_authed(&app.router, "GET", "/api/employees", None, &staff).await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert!(
        body["data"]["employees"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == id)
    );

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/employees/{id}"),
        None,
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["id"], id);
}

// === Update, optimistic locking ===

#[tokio::test]
async fn update_employee_applies_partial_changes() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, employee) = create_employee_via_api(&app.router, &token).await;

    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/employees/{id}"),
        Some(json!({ "defaultSplitValue": 30.0 })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["defaultSplitValue"], 30.0);
    // Untouched fields keep their existing value.
    assert_eq!(body["data"]["name"], employee["name"]);
}

#[tokio::test]
async fn update_employee_version_conflict() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _) = create_employee_via_api(&app.router, &token).await;

    let request = Request::builder()
        .method("PATCH")
        .uri(format!("/api/employees/{id}"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header("If-Match", "999")
        .body(Body::from(json!({ "name": "Renamed" }).to_string()))
        .unwrap();
    let (status, body) = execute(&app.router, request).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["code"], "VERSION_CONFLICT");
}

// === Delete, EMPLOYEE_HAS_LOGIN guard ===

#[tokio::test]
async fn delete_employee_without_login_succeeds() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _) = create_employee_via_api(&app.router, &token).await;

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/employees/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
}

#[tokio::test]
async fn delete_employee_with_login_is_blocked() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_id, employee) = create_employee_via_api(&app.router, &token).await;
    let employee_key = employee["key"].as_str().unwrap();

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Linked Login",
            "username": format!("linked-{}", &Uuid::new_v4().simple().to_string()[..12]),
            "password": "Password123!",
            "role": "staff",
            "employeeKey": employee_key,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");

    // Look the employee back up by id via the list endpoint to get its
    // Mongo id for the delete call (create_employee_via_api already has it).
    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/employees/{}", employee["id"].as_str().unwrap()),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["code"], "EMPLOYEE_HAS_LOGIN");
}

#[tokio::test]
async fn get_employee_reflects_linked_login() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, employee) = create_employee_via_api(&app.router, &token).await;
    let employee_key = employee["key"].as_str().unwrap();
    let username = format!("has-login-{}", &Uuid::new_v4().simple().to_string()[..12]);

    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Has Login",
            "username": username,
            "password": "Password123!",
            "role": "staff",
            "employeeKey": employee_key,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/employees/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["login"]["username"], username);
    assert_eq!(body["data"]["login"]["role"], "staff");
}

// === Batch delete ===

#[tokio::test]
async fn batch_delete_employees_skips_ones_with_a_login() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id1, _) = create_employee_via_api(&app.router, &token).await;
    let (id2, employee2) = create_employee_via_api(&app.router, &token).await;

    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Linked",
            "username": format!("batch-{}", &Uuid::new_v4().simple().to_string()[..12]),
            "password": "Password123!",
            "role": "staff",
            "employeeKey": employee2["key"].as_str().unwrap(),
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        "/api/employees/batch",
        Some(json!({ "ids": [id1, id2] })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["deletedCount"], 1);
}
