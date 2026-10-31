//! Shared setup for the `sessions_test` tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::Value;

use simplebash_pos_backend::domain::cash_categories::category_key;
use simplebash_pos_backend::domain::sessions::CashMovementType;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

/// Key of a built-in cash category, the way the app derives it.
pub(crate) fn cat(movement_type: CashMovementType, name: &str) -> String {
    category_key(movement_type, name)
}

pub(crate) async fn execute(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("the text is valid JSON")
    };
    (status, json)
}

pub(crate) async fn send_authed(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    let req = if let Some(value) = body {
        builder = builder.header(CONTENT_TYPE, "application/json");
        builder
            .body(Body::from(value.to_string()))
            .expect("the request is well formed")
    } else {
        builder
            .body(Body::empty())
            .expect("the request is well formed")
    };
    execute(router, req).await
}

pub(crate) async fn send_authed_with_device(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
    device_id: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    if let Some(did) = device_id {
        builder = builder.header("x-device-id", did);
    }
    let req = if let Some(value) = body {
        builder = builder.header(CONTENT_TYPE, "application/json");
        builder
            .body(Body::from(value.to_string()))
            .expect("the request is well formed")
    } else {
        builder
            .body(Body::empty())
            .expect("the request is well formed")
    };
    execute(router, req).await
}

pub(crate) fn mint_token_for(
    config: &simplebash_pos_backend::core::config::Config,
    user_id_or_username: &str,
    role: Option<Role>,
    permissions: &[&str],
) -> String {
    let claims = simplebash_pos_backend::core::middleware::auth::Claims {
        sub: user_id_or_username.to_string(),
        exp: 9_999_999_999,
        role,
        permissions: permissions.iter().map(|p| p.to_string()).collect(),
        tid: None,
        scope: None,
        did: None,
        name: Some("Test Cashier".to_string()),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .expect("the token encodes")
}
