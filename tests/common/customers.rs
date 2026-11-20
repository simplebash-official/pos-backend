//! Registers customers for the tests of records (repair jobs, print jobs) that must name one.

use super::{TestApp, mint_token};
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::domain::users::Role;
use tower::ServiceExt;

/// Registers a customer through the API and returns its key.
#[allow(dead_code)]
pub async fn register_customer(app: &TestApp, name: &str, phone: &str) -> String {
    let token = mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/customers")
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(
            json!({ "name": name, "primaryPhone": phone }).to_string(),
        ))
        .expect("the request is well formed");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let body: Value = serde_json::from_slice(&bytes).expect("the text is valid JSON");
    assert_eq!(status, StatusCode::OK, "registering a customer: {body}");
    body["data"]["key"]
        .as_str()
        .expect("the customer has a key")
        .to_string()
}
