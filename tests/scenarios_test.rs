// Builds the real router against a real MongoDB (via `common::spawn_app()`,
// targeting the isolated test database) — the slowest but most realistic
// of the three test tiers (see CLAUDE.md). Use for anything that actually
// reads/writes Mongo; `tests/inventory_test.rs` is the larger example of
// this style.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn health_route_returns_success_format() {
    let app = common::spawn_app().await;

    let response = app
        .router
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["status"], "ok");
    assert_eq!(json["message"], "Service is healthy");
}
