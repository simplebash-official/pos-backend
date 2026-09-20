// Covers `modules::users::service::reset_admin_credentials` (the function
// `src/bin/reset_admin.rs` calls) — the only way to rotate the single Admin
// account's email/password, since it's unreachable through any HTTP route.
// Uses `spawn_app_sqlite()` (per-test isolated DB), never the shared Mongo
// test DB, since this touches the single-Admin invariant the same way
// `users_test.rs`'s `single_admin_invariant_and_admin_invisible_via_users_api`
// has to carefully isolate itself for.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;
use simplebash_pos_backend::{core::error::AppError, modules::users::service, seeds};
use tower::ServiceExt;

async fn send(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request = if let Some(json_body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        builder
            .body(Body::from(serde_json::to_vec(&json_body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

#[tokio::test]
async fn reset_admin_credentials_rotates_email_and_password() {
    let app = common::spawn_app_sqlite().await;
    seeds::admin::seed_admin(&app.db_handle, None, None, None)
        .await
        .expect("seed_admin should create the initial admin");

    let updated = service::reset_admin_credentials(&app.db_handle, "new-admin@pos.com", "new@1234")
        .await
        .expect("reset_admin_credentials should succeed against an existing admin");
    assert_eq!(updated.email, "new-admin@pos.com");

    // Old credentials no longer work.
    let (old_status, _) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": "admin@pos.com", "password": "admin@1234" })),
    )
    .await;
    assert_eq!(old_status, StatusCode::UNAUTHORIZED);

    // New credentials log in as the same (now-renamed) admin.
    let (new_status, new_res) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({ "email": "new-admin@pos.com", "password": "new@1234" })),
    )
    .await;
    assert_eq!(new_status, StatusCode::OK, "login failed: {new_res}");
    assert_eq!(new_res["data"]["user"]["email"], "new-admin@pos.com");
    assert_eq!(new_res["data"]["user"]["role"], "admin");
}

#[tokio::test]
async fn reset_admin_credentials_errors_when_no_admin_exists() {
    let app = common::spawn_app_sqlite().await;

    let result =
        service::reset_admin_credentials(&app.db_handle, "x@example.com", "password123").await;

    assert!(
        matches!(result, Err(AppError::NotFound { .. })),
        "expected NotFound, got {result:?}"
    );
}
