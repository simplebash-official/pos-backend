mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use jana2u_pos_backend::domain::users::Role;
use tower::ServiceExt;
use uuid::Uuid;

async fn send_with_headers(
    router: &axum::Router,
    token: &str,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    extra_headers: Vec<(&str, &str)>,
) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));

    for (k, v) in extra_headers {
        builder = builder.header(k, v);
    }

    let request = if let Some(json_body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&json_body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let json: serde_json::Value = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null)
    };

    (status, headers, json)
}

#[tokio::test]
async fn idempotency_replay_and_body_mismatch_validation() {
    let app = common::spawn_app().await;
    let token = common::mint_token(&app.config, Some(Role::Admin), &[]);
    let idem_key = format!("idem-{}", Uuid::new_v4());

    let payload = serde_json::json!({ "blockSize": 10 });

    // 1. Initial request with Idempotency-Key
    let (status1, headers1, json1) = send_with_headers(
        &app.router,
        &token,
        "POST",
        "/api/sequences/repair/reserve",
        Some(payload.clone()),
        vec![("idempotency-key", &idem_key)],
    )
    .await;

    assert_eq!(status1, StatusCode::OK, "first request: {json1}");
    assert!(headers1.get("idempotency-replayed").is_none());
    let start1 = json1["data"]["start"].as_i64().unwrap();

    // 2. Replay with identical payload and same Idempotency-Key -> cached response with idempotency-replayed: true
    let (status2, headers2, json2) = send_with_headers(
        &app.router,
        &token,
        "POST",
        "/api/sequences/repair/reserve",
        Some(payload),
        vec![("idempotency-key", &idem_key)],
    )
    .await;

    assert_eq!(status2, StatusCode::OK, "replayed request: {json2}");
    assert_eq!(
        headers2
            .get("idempotency-replayed")
            .unwrap()
            .to_str()
            .unwrap(),
        "true"
    );
    let start2 = json2["data"]["start"].as_i64().unwrap();
    assert_eq!(
        start1, start2,
        "replayed response must match original exactly"
    );

    // 3. Request with same Idempotency-Key but different payload -> 422 IDEMPOTENCY_KEY_REUSED
    let different_payload = serde_json::json!({ "blockSize": 25 });
    let (status3, _, json3) = send_with_headers(
        &app.router,
        &token,
        "POST",
        "/api/sequences/repair/reserve",
        Some(different_payload),
        vec![("idempotency-key", &idem_key)],
    )
    .await;

    assert_eq!(
        status3,
        StatusCode::UNPROCESSABLE_ENTITY,
        "mismatched payload: {json3}"
    );
    assert_eq!(json3["code"], "IDEMPOTENCY_KEY_REUSED");
}
