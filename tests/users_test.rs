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
    core::{
        constants::{codes, roles},
        error::AppError,
    },
    domain::users::{CreateUserRequest, Role},
    modules::users::service::create_user,
};
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

/// A caller authenticated as Staff — a real role, but Staff's default
/// permission set has neither `users:manage` nor `users:manage:staff`, so
/// every `users` write/read route should reject this token with 403.
fn staff_token(config: &simplebash_pos_backend::core::config::Config) -> String {
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

async fn create_user_via_api(
    router: &axum::Router,
    token: &str,
    role: &str,
) -> (String, String, Value) {
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
// Create — hierarchy: Admin creates Manager/Staff, Manager creates Staff only
// ============================================================================

#[tokio::test]
async fn admin_can_create_manager_and_staff_users() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (_, _, manager) = create_user_via_api(&app.router, &token, "manager").await;
    assert_eq!(manager["role"], "manager");
    assert!(manager.get("passwordHash").is_none());
    assert!(manager.get("password").is_none());

    let (_, _, staff) = create_user_via_api(&app.router, &token, "staff").await;
    assert_eq!(staff["role"], "staff");
}

#[tokio::test]
async fn admin_cannot_create_admin_account_via_api() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(sample_user_payload("admin")),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn manager_can_create_staff_but_not_manager_or_admin() {
    let app = common::spawn_app().await;
    let token = manager_token(&app.config);

    let (_, _, staff) = create_user_via_api(&app.router, &token, "staff").await;
    assert_eq!(staff["role"], "staff");

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(sample_user_payload("manager")),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(sample_user_payload("admin")),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
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
async fn caller_without_users_permission_gets_403_creating_user() {
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

/// Covers both the single-Admin invariant (enforced in
/// `users::service::create_user`, the raw function `src/bin/seed_admin.rs`
/// calls — the HTTP API can never reach it with `role: "admin"` at all,
/// since `create_user_for_caller` rejects that role for every caller, see
/// `admin_cannot_create_admin_account_via_api` above) and that an Admin
/// account is invisible through `/users` even to itself.
///
/// Both assertions are folded into one test rather than split, and it
/// clears any pre-existing Admin documents from the (shared, not reset
/// between `cargo test` runs) test database before it starts: this is the
/// only test that creates a `Role::Admin` account directly, so nothing else
/// can race with its cleanup, and without it a leftover Admin from an
/// earlier run would make "creating the first Admin" spuriously fail.
#[tokio::test]
async fn single_admin_invariant_and_admin_invisible_via_users_api() {
    let app = common::spawn_app().await;
    app.db
        .collection::<mongodb::bson::Document>("users")
        .delete_many(mongodb::bson::doc! { "role": "admin" })
        .await
        .expect("failed to clear pre-existing admin accounts before test");

    let email = format!("only-admin-{}@example.com", Uuid::new_v4());
    let first = create_user(
        &app.db_handle,
        CreateUserRequest {
            name: "Only Admin".to_string(),
            email: email.clone(),
            password: "Password123!".to_string(),
            role: Role::Admin,
            employee_key: None,
        },
    )
    .await;
    assert!(first.is_ok(), "{first:?}");

    let second = create_user(
        &app.db_handle,
        CreateUserRequest {
            name: "Second Admin".to_string(),
            email: format!("second-admin-{}@example.com", Uuid::new_v4()),
            password: "Password123!".to_string(),
            role: Role::Admin,
            employee_key: None,
        },
    )
    .await;
    assert!(
        matches!(second, Err(AppError::Custom { ref code, .. }) if code == codes::ADMIN_ALREADY_EXISTS),
        "expected ADMIN_ALREADY_EXISTS, got {second:?}"
    );

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": "Password123!" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let admin_id = body["data"]["user"]["id"].as_str().unwrap().to_string();

    let caller = admin_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/users/{admin_id}"),
        None,
        &caller,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "an Admin account must be invisible through /users, even to itself: {body}"
    );
    assert_eq!(body["code"], "USER_NOT_FOUND");
}

// ============================================================================
// List — Admin sees Manager+Staff, Manager sees Staff only
// ============================================================================

#[tokio::test]
async fn list_users_requires_users_permission() {
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
async fn manager_list_only_shows_staff_accounts() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (_, _, manager_account) = create_user_via_api(&app.router, &admin, "manager").await;
    let (_, _, staff_account) = create_user_via_api(&app.router, &admin, "staff").await;

    let manager = manager_token(&app.config);
    let (status, body) = send_authed(&app.router, "GET", "/api/users", None, &manager).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let users = body["data"]["users"].as_array().unwrap();

    assert!(
        users.iter().all(|u| u["role"] == "staff"),
        "Manager's list must only contain Staff accounts: {users:?}"
    );
    assert!(
        users.iter().any(|u| u["email"] == staff_account["email"]),
        "expected the created Staff account to appear: {users:?}"
    );
    assert!(
        !users.iter().any(|u| u["email"] == manager_account["email"]),
        "a Manager account must never appear in another Manager's list: {users:?}"
    );
}

// ============================================================================
// Get — 404s (not 403) outside the caller's manageable scope
// ============================================================================

#[tokio::test]
async fn admin_can_get_manager_and_staff_by_id() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _, created) = create_user_via_api(&app.router, &token, "staff").await;

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
async fn manager_cannot_get_manager_or_admin_accounts() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (other_manager_id, _, _) = create_user_via_api(&app.router, &admin, "manager").await;

    let manager = manager_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/users/{other_manager_id}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "USER_NOT_FOUND");
}

// ============================================================================
// Update — role changes are also bounded by the hierarchy
// ============================================================================

#[tokio::test]
async fn admin_update_user_changes_role_name_email() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _, _) = create_user_via_api(&app.router, &token, "staff").await;

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
async fn admin_cannot_promote_a_user_to_admin() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _, _) = create_user_via_api(&app.router, &token, "staff").await;

    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/users/{id}"),
        Some(json!({ "role": "admin" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn manager_cannot_promote_staff_to_manager() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (staff_id, _, _) = create_user_via_api(&app.router, &admin, "staff").await;

    let manager = manager_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/users/{staff_id}"),
        Some(json!({ "role": "manager" })),
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
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

// ============================================================================
// Delete — same 404-outside-scope rule; no self-delete guard needed since no
// role is ever in its own manageable set (Admin doesn't manage Admin,
// Manager doesn't manage Manager), so self-delete already 404s naturally.
// ============================================================================

#[tokio::test]
async fn admin_can_delete_staff_account() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _, _) = create_user_via_api(&app.router, &token, "staff").await;

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
async fn manager_can_delete_staff_but_not_manager_accounts() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (staff_id, _, _) = create_user_via_api(&app.router, &admin, "staff").await;
    let (other_manager_id, _, _) = create_user_via_api(&app.router, &admin, "manager").await;

    let manager = manager_token(&app.config);
    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/users/{staff_id}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/users/{other_manager_id}"),
        None,
        &manager,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "USER_NOT_FOUND");
}

// ============================================================================
// Employee-linked logins
// ============================================================================

async fn create_employee_via_api(router: &axum::Router, token: &str) -> Value {
    let (status, body) = send_authed(
        router,
        "POST",
        "/api/employees",
        Some(json!({
            "name": format!("Test Employee {}", Uuid::new_v4()),
            "phone": "0771234567",
            "role": "technician",
            "defaultSplitType": "percentage",
            "defaultSplitValue": 20.0,
        })),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"].clone()
}

#[tokio::test]
async fn creating_user_with_employee_key_links_it() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let employee = create_employee_via_api(&app.router, &token).await;
    let employee_key = employee["key"].as_str().unwrap();

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Linked User",
            "email": format!("linked-{}@example.com", Uuid::new_v4()),
            "password": "Password123!",
            "role": "staff",
            "employeeKey": employee_key,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["employeeKey"], employee_key);
}

#[tokio::test]
async fn creating_user_with_unresolvable_employee_key_is_not_found() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Orphan Login",
            "email": format!("orphan-{}@example.com", Uuid::new_v4()),
            "password": "Password123!",
            "role": "staff",
            "employeeKey": "emp_does_not_exist",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "EMPLOYEE_NOT_FOUND");
}

#[tokio::test]
async fn creating_second_login_for_already_linked_employee_is_rejected() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let employee = create_employee_via_api(&app.router, &token).await;
    let employee_key = employee["key"].as_str().unwrap();

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "First Login",
            "email": format!("first-{}@example.com", Uuid::new_v4()),
            "password": "Password123!",
            "role": "staff",
            "employeeKey": employee_key,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Second Login",
            "email": format!("second-{}@example.com", Uuid::new_v4()),
            "password": "Password123!",
            "role": "staff",
            "employeeKey": employee_key,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "EMPLOYEE_ALREADY_HAS_LOGIN");
}
