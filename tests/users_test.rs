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

fn admin_token(config: &jana2u_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

/// A caller authenticated as Staff — a real role, but Staff's default
/// permission set has no `users:manage`, so every `users` write/read route
/// should reject this token with 403.
fn staff_token(config: &jana2u_pos_backend::core::config::Config) -> String {
    common::mint_token(
        config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    )
}

fn sample_user_payload(role: &str) -> Value {
    json!({
        "name": "Test User",
        "email": format!("user-{}@example.com", Uuid::new_v4()),
        "password": "Password123!",
        "role": role,
    })
}

async fn create_user(router: &axum::Router, token: &str, role: &str) -> (String, String, Value) {
    let (status, body) = send_authed(
        router,
        "POST",
        "/api/users",
        Some(sample_user_payload(role)),
        token,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "failed to create test user: {body}"
    );
    let id = body["data"]["id"].as_str().unwrap().to_string();
    let key = body["data"]["key"].as_str().unwrap().to_string();
    (id, key, body["data"].clone())
}

// ============================================================================
// User CRUD
// ============================================================================

#[tokio::test]
async fn admin_can_create_manager_and_staff_users() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (_, _, manager) = create_user(&app.router, &token, "manager").await;
    assert_eq!(manager["role"], "manager");
    assert!(manager.get("passwordHash").is_none());
    assert!(manager.get("password").is_none());

    let (_, _, staff) = create_user(&app.router, &token, "staff").await;
    assert_eq!(staff["role"], "staff");
}

#[tokio::test]
async fn creating_user_with_duplicate_email_returns_409() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let payload = sample_user_payload("staff");
    let (status, _) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(payload.clone()),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) =
        send_authed(&app.router, "POST", "/api/users", Some(payload), &token).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "EMAIL_ALREADY_EXISTS");
}

#[tokio::test]
async fn create_user_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send(
        &app.router,
        "POST",
        "/api/users",
        Some(sample_user_payload("staff")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn caller_without_users_manage_permission_gets_403_creating_user() {
    let app = common::spawn_app().await;
    let token = staff_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(sample_user_payload("staff")),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn list_users_requires_users_manage_permission() {
    let app = common::spawn_app().await;
    let staff = staff_token(&app.config);

    let (status, _) = send_authed(&app.router, "GET", "/api/users", None, &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let admin = admin_token(&app.config);
    let (status, body) = send_authed(&app.router, "GET", "/api/users", None, &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["users"].is_array());
}

#[tokio::test]
async fn get_user_by_id() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _, created) = create_user(&app.router, &token, "staff").await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/users/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["email"], created["email"]);
}

#[tokio::test]
async fn update_user_changes_role_name_email() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _, _) = create_user(&app.router, &token, "staff").await;

    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/users/{id}"),
        Some(json!({ "name": "Updated Name", "role": "manager" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["name"], "Updated Name");
    assert_eq!(body["data"]["role"], "manager");
}

#[tokio::test]
async fn updated_password_takes_effect_on_next_login() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let email = format!("pwtest-{}@example.com", Uuid::new_v4());

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Password Test User",
            "email": &email,
            "password": "OriginalPass123",
            "role": "staff",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let user_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, _) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": "OriginalPass123" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/users/{user_id}"),
        Some(json!({ "password": "NewPassword456" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, _) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": "OriginalPass123" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "old password should no longer work"
    );

    let (status, _) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": "NewPassword456" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "new password should now work");
}

#[tokio::test]
async fn delete_user_removes_account() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _, _) = create_user(&app.router, &token, "staff").await;

    let (status, _) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/users/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_authed(
        &app.router,
        "GET",
        &format!("/api/users/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cannot_delete_own_account_returns_error() {
    let app = common::spawn_app().await;
    let bootstrap_token = admin_token(&app.config);

    // `common::mint_token`'s Claims always carry a hardcoded `sub:
    // "test-user"`, which never matches a real user's `ObjectId` — so the
    // self-delete guard can only be exercised with a token whose `sub` is a
    // real account, obtained by actually logging in as one.
    let email = format!("self-delete-{}@example.com", Uuid::new_v4());
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Self Delete Admin",
            "email": &email,
            "password": "SelfDelete123",
            "role": "admin",
        })),
        &bootstrap_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let user_id = body["data"]["id"].as_str().unwrap().to_string();

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": "SelfDelete123" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let own_token = body["data"]["token"].as_str().unwrap().to_string();

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/users/{user_id}"),
        None,
        &own_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "CANNOT_DELETE_SELF");
}
