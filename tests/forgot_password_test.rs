//! Integration tests for the Admin-only "Forgot Password" feature.
//!
//! Tests the complete lifecycle:
//! 1. Requesting a reset token with admin username, admin email, and shop profile email.
//! 2. Timing attack defense when requesting reset for non-existent accounts.
//! 3. Invalidating previous tokens when a new reset is requested.
//! 4. Verifying valid, invalid, expired, and already-used reset tokens.
//! 5. Resetting password with complexity validation.
//! 6. Prevention of token reuse.
//! 7. Sign in with the new password succeeds; sign in with the old password fails.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use sha2::{Digest, Sha256};
use simplebash_pos_backend::core::constants::codes;
use simplebash_pos_backend::seeds;
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
            .body(Body::from(
                serde_json::to_vec(&json_body).expect("the value serializes to JSON"),
            ))
            .expect("the value serializes to JSON")
    } else {
        builder
            .body(Body::empty())
            .expect("the value serializes to JSON")
    };

    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the request succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body reads completely");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

fn hash_token(raw_token: &str) -> String {
    let hash = Sha256::digest(raw_token.as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::test]
async fn forgot_password_full_lifecycle() {
    let app = common::spawn_app_sqlite().await;
    seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("seed_admin succeeds");

    let pool = app.db_handle.as_sqlite().expect("sqlite pool");

    // 1. Timing defense: requesting reset for an unknown user returns success without creating a token
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/forgot-password",
        Some(json!({
            "identifier": "nonexistent_user@example.com"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["success"], true);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens")
        .fetch_one(pool)
        .await
        .expect("count query succeeds");
    assert_eq!(
        count, 0,
        "No token should be created for unknown identifier"
    );

    // 2. Request reset using the admin username ("admin")
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/forgot-password",
        Some(json!({
            "identifier": "admin"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["success"], true);

    let tokens_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens")
        .fetch_one(pool)
        .await
        .expect("count query succeeds");
    assert_eq!(tokens_count, 1, "One token should be created for admin");

    // 3. Request reset again -> previous token should be invalidated (marked used)
    let (status, _) = send(
        &app.router,
        "POST",
        "/api/auth/forgot-password",
        Some(json!({
            "identifier": "admin"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let active_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens WHERE used_at IS NULL")
            .fetch_one(pool)
            .await
            .expect("count query succeeds");
    assert_eq!(active_count, 1, "Only one active token should exist");

    let used_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens WHERE used_at IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("count query succeeds");
    assert_eq!(used_count, 1, "Prior token should have been invalidated");

    // 4. Update the active token in the database to have a known raw token's hash
    let raw_token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let token_hash = hash_token(raw_token);
    sqlx::query("UPDATE password_reset_tokens SET token_hash = $1 WHERE used_at IS NULL")
        .bind(&token_hash)
        .execute(pool)
        .await
        .expect("update token_hash succeeds");

    // Verify reset token: invalid token returns 400
    let (status, resp) = send(
        &app.router,
        "GET",
        "/api/auth/verify-reset-token?token=nonexistent_token_hex",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], codes::INVALID_OR_EXPIRED_RESET_TOKEN);

    // Verify reset token: valid token returns 200
    let (status, resp) = send(
        &app.router,
        "GET",
        &format!("/api/auth/verify-reset-token?token={raw_token}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["success"], true);

    // Reset password: too short (< 8 chars) fails validation
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/reset-password",
        Some(json!({
            "token": raw_token,
            "newPassword": "short"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], codes::VALIDATION_ERROR);

    // Reset password: valid token + valid password succeeds
    let new_password = "BrandNewSecureAdminPass123!";
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/reset-password",
        Some(json!({
            "token": raw_token,
            "newPassword": new_password
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["success"], true);

    // 5. Token reuse prevention: token is now marked used
    let (status, resp) = send(
        &app.router,
        "GET",
        &format!("/api/auth/verify-reset-token?token={raw_token}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], codes::INVALID_OR_EXPIRED_RESET_TOKEN);

    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/reset-password",
        Some(json!({
            "token": raw_token,
            "newPassword": "AnotherPassword456!"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], codes::INVALID_OR_EXPIRED_RESET_TOKEN);

    // 6. Sign in with the old password fails (401)
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({
            "username": "admin",
            "password": "admin@1234"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(resp["success"], false);

    // 7. Sign in with the new password succeeds (200)
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/login",
        Some(json!({
            "username": "admin",
            "password": new_password
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["success"], true);
    assert!(resp["data"]["token"].is_string());
}

#[tokio::test]
async fn forgot_password_matching_email_and_expired_tokens() {
    let app = common::spawn_app_sqlite().await;
    seeds::admin::seed_admin(&app.db_handle, &app.config.seed_admin)
        .await
        .expect("seed_admin succeeds");

    let pool = app.db_handle.as_sqlite().expect("sqlite pool");
    let admin_email = "admin-owner@simplebashpos.com";

    // Set admin email in DB
    sqlx::query("UPDATE users SET email = $1 WHERE username = 'admin'")
        .bind(admin_email)
        .execute(pool)
        .await
        .expect("update email succeeds");

    // Request reset using admin email
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/forgot-password",
        Some(json!({
            "identifier": admin_email
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["success"], true);

    // Verify token was inserted
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens WHERE used_at IS NULL")
            .fetch_one(pool)
            .await
            .expect("count query succeeds");
    assert_eq!(count, 1);

    // Expire the token by setting expires_at into the past
    let raw_token = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let token_hash = hash_token(raw_token);
    sqlx::query(
        "UPDATE password_reset_tokens SET token_hash = $1, expires_at = '2020-01-01T00:00:00Z' WHERE used_at IS NULL"
    )
    .bind(&token_hash)
    .execute(pool)
    .await
    .expect("update expires_at succeeds");

    // Verify expired token fails
    let (status, resp) = send(
        &app.router,
        "GET",
        &format!("/api/auth/verify-reset-token?token={raw_token}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], codes::INVALID_OR_EXPIRED_RESET_TOKEN);

    // Attempting reset with expired token fails
    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/reset-password",
        Some(json!({
            "token": raw_token,
            "newPassword": "ValidPassword123!"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(resp["code"], codes::INVALID_OR_EXPIRED_RESET_TOKEN);

    // Request reset using shop profile email
    let shop_email = "shop-contact@simplebashpos.com";
    sqlx::query("UPDATE shop_profiles SET email = $1 WHERE key = 'main'")
        .bind(shop_email)
        .execute(pool)
        .await
        .expect("update shop_profiles email succeeds");

    let (status, resp) = send(
        &app.router,
        "POST",
        "/api/auth/forgot-password",
        Some(json!({
            "identifier": shop_email
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["success"], true);

    let active_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens WHERE used_at IS NULL")
            .fetch_one(pool)
            .await
            .expect("count query succeeds");
    assert_eq!(active_count, 1);
}
