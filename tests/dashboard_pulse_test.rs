// The dashboard's groupings over HTTP, on SQLite. The grouping rules themselves are unit-tested in
// `modules::reports::service::pulse`; this checks the route: sign-in, the reply shape and that a
// shop with nothing in it gets empty groups rather than an error.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header::AUTHORIZATION},
};
use serde_json::Value;
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
use tower::ServiceExt;

async fn get(app: &common::TestApp, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("GET")
        .uri("/api/reports/dashboard-pulse");
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn an_empty_shop_gets_empty_groups_and_a_signed_out_caller_is_refused() {
    let app = common::spawn_app_sqlite().await;
    let (status, _) = get(&app, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let admin = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    let (status, body) = get(&app, Some(&admin)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["activeRepairsCount"], 0);
    assert_eq!(data["uncollectedReadyValueCents"], 0);
    assert_eq!(data["repairPipeline"].as_array().unwrap().len(), 4);
    assert_eq!(data["repairPipeline"][0]["stage"], "received");
    assert_eq!(data["printPipeline"][3]["stage"], "delivered");
    assert_eq!(data["technicians"], serde_json::json!([]));
    assert_eq!(data["recentActivity"], serde_json::json!([]));

    // Without employee or stock access those groups stay empty instead of failing the dashboard.
    let nobody = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = get(&app, Some(&nobody)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["lowStock"], serde_json::json!([]));
}
