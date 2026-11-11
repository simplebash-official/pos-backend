//! Test-harness pieces: tokens.

use super::test_app::TestApp;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::json;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::middleware::auth::Claims;
use simplebash_pos_backend::domain::users::Role;

/// Mints a JWT signed with the test app's own `jwt_secret`, carrying the
/// given role and permission set — shared across every test file that gates
/// a request behind `CurrentUser`/`AdminUser`/a specific permission without
/// exercising the real login flow (login itself is only hand-tested
/// end-to-end in `tests/auth_test.rs`).
#[allow(dead_code)]
pub fn mint_token(config: &Config, role: Option<Role>, permissions: &[&str]) -> String {
    let claims = Claims {
        sub: "test-user".to_string(),
        exp: 9_999_999_999,
        role,
        permissions: permissions.iter().map(|p| p.to_string()).collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some("Test User".to_string()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes")
}

/// Like `mint_token`, but for a multi-tenant deployment: the token carries the
/// tenant it was issued for (`tid`). `None` mints a token with no tenant.
#[allow(dead_code)]
pub fn mint_token_for_tenant(
    config: &Config,
    role: Option<Role>,
    permissions: &[&str],
    tid: Option<&str>,
) -> String {
    let claims = Claims {
        sub: "test-user".to_string(),
        exp: 9_999_999_999,
        role,
        permissions: permissions.iter().map(|p| p.to_string()).collect(),
        tid: tid.map(str::to_string),
        scope: None,
        did: None,
        name: Some("Test User".to_string()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes")
}

/// A token for a registered sync device of `tid` (scope `device`, claim `did`).
#[allow(dead_code)]
pub fn mint_device_token(config: &Config, tid: &str, device_id: &str) -> String {
    let claims = Claims {
        sub: format!("device-{device_id}"),
        exp: 9_999_999_999,
        role: Some(Role::Admin),
        permissions: Vec::new(),
        tid: Some(tid.to_string()),
        scope: Some("device".to_string()),
        did: Some(device_id.to_string()),
        name: None,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes")
}

/// A device token for `device_id` of `tid`, registered with the cloud as an
/// app new enough to sync (`POST /sync/devices/register`), the way every desktop
/// agent starts. Push, pull and snapshot refuse a device that never registered.
#[allow(dead_code)]
pub async fn registered_device_token(app: &TestApp, tid: &str, device_id: &str) -> String {
    use tower::ServiceExt;
    let token = mint_device_token(&app.config, tid, device_id);
    let body = json!({ "deviceId": device_id, "deviceName": "Test PC", "appVersion": "1.31.0" });
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/api/sync/devices/register")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(axum::body::Body::from(body.to_string()))
        .expect("the request is well formed");
    // Not asserted: a revoked device is refused here too, and a test of that
    // checks the refusal of its own push/pull. A failed registration shows up
    // there as APP_UPDATE_REQUIRED.
    app.router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    token
}
