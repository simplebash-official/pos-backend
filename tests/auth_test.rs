mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jana2u_pos_backend::{
    core::{constants::roles, middleware::auth::Claims},
    domain::users::Role,
};
use jsonwebtoken::{DecodingKey, Validation, decode};
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

/// Creates a user via the real (admin-provisioned) API and returns
/// `(id, email, password)`.
async fn create_user(router: &axum::Router, admin: &str, role: &str) -> (String, String, String) {
    let email = format!("auth-test-{}@example.com", Uuid::new_v4());
    let password = "Password123!".to_string();
    let (status, body) = send_authed(
        router,
        "POST",
        "/api/users",
        Some(json!({
            "name": "Auth Test User",
            "email": &email,
            "password": &password,
            "role": role,
        })),
        admin,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "failed to create test user: {body}"
    );
    let id = body["data"]["id"].as_str().unwrap().to_string();
    (id, email, password)
}

// ============================================================================
// Login
// ============================================================================

#[tokio::test]
async fn login_with_valid_credentials_returns_token_and_user_with_expected_role_and_permissions() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (_, email, password) = create_user(&app.router, &admin, "staff").await;

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": &password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let token = body["data"]["token"].as_str().unwrap();
    assert!(body["data"]["expiresIn"].as_i64().unwrap() > 0);
    assert_eq!(body["data"]["user"]["email"], email);
    assert_eq!(body["data"]["user"]["role"], "staff");

    let decoded = decode::<Claims>(
        token,
        &DecodingKey::from_secret(app.config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .expect("issued token must decode with the app's own secret");
    assert_eq!(decoded.claims.role, Some(Role::Staff));
    let expected: Vec<String> = roles::default_permissions(Role::Staff)
        .iter()
        .map(|p| p.to_string())
        .collect();
    let mut actual = decoded.claims.permissions.clone();
    actual.sort();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort();
    assert_eq!(actual, expected_sorted);
}

#[tokio::test]
async fn login_with_wrong_password_returns_401_invalid_credentials() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (_, email, _) = create_user(&app.router, &admin, "staff").await;

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": "WrongPassword" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], "INVALID_CREDENTIALS");
}

#[tokio::test]
async fn login_with_nonexistent_email_returns_same_401_invalid_credentials() {
    let app = common::spawn_app().await;

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": format!("nobody-{}@example.com", Uuid::new_v4()), "password": "whatever123" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(
        body["code"], "INVALID_CREDENTIALS",
        "a nonexistent email must fail with the same code as a wrong password, to avoid user enumeration"
    );
}

#[tokio::test]
async fn login_with_deactivated_account_returns_401_user_inactive() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (id, email, password) = create_user(&app.router, &admin, "staff").await;

    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/users/{id}"),
        Some(json!({ "isActive": false })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": &password })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], "USER_INACTIVE");
}

// ============================================================================
// /me
// ============================================================================

#[tokio::test]
async fn me_returns_current_user_info() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (_, email, password) = create_user(&app.router, &admin, "manager").await;

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": &password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["data"]["token"].as_str().unwrap().to_string();

    let (status, body) = send_authed(&app.router, "GET", "/api/auth/me", None, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["email"], email);
    assert_eq!(body["data"]["role"], "manager");
}

#[tokio::test]
async fn me_without_token_returns_401() {
    let app = common::spawn_app().await;
    let (status, _) = send(&app.router, "GET", "/api/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_for_account_deactivated_after_token_issuance_returns_401() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (id, email, password) = create_user(&app.router, &admin, "staff").await;

    let (status, body) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": &password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = body["data"]["token"].as_str().unwrap().to_string();

    // The token is still cryptographically valid — deactivate the account
    // out from under it and confirm /me (which re-reads from Mongo) is the
    // one place that catches this, since there is no revocation infra.
    let (status, _) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/users/{id}"),
        Some(json!({ "isActive": false })),
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_authed(&app.router, "GET", "/api/auth/me", None, &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], "USER_INACTIVE");
}

// ============================================================================
// Login sessions
// ============================================================================

#[tokio::test]
async fn login_records_a_session_visible_via_sessions_endpoint() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (_, email, password) = create_user(&app.router, &admin, "staff").await;

    let (status, _) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": &email, "password": &password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send_authed(&app.router, "GET", "/api/auth/sessions", None, &admin).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert!(
        sessions
            .iter()
            .any(|s| s["email"] == email && s["role"] == "staff"),
        "expected a session entry for {email}: {sessions:?}"
    );
}

#[tokio::test]
async fn sessions_requires_sessions_view_permission() {
    let app = common::spawn_app().await;
    let staff = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );

    let (status, body) = send_authed(&app.router, "GET", "/api/auth/sessions", None, &staff).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[tokio::test]
async fn sessions_filter_by_user_id() {
    let app = common::spawn_app().await;
    let admin = admin_token(&app.config);
    let (id_a, email_a, password_a) = create_user(&app.router, &admin, "staff").await;
    let (_, email_b, password_b) = create_user(&app.router, &admin, "staff").await;

    for (email, password) in [(&email_a, &password_a), (&email_b, &password_b)] {
        let (status, _) = send(
            &app.router,
            "POST",
            "/api/auth/login",
            Some(json!({ "email": email, "password": password })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/auth/sessions?userId={id_a}"),
        None,
        &admin,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert!(!sessions.is_empty());
    assert!(sessions.iter().all(|s| s["email"] == email_a));
}
