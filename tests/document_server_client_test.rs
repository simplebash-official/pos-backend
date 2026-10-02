// Covers `DocumentServerClient::wait_until_ready` (the startup readiness
// check `main.rs` runs before serving traffic) in isolation, against a
// throwaway in-process mock server — no router, no Mongo needed, same
// no-live-dependency style as `tests/response_format_test.rs`.

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::get};
use serde_json::json;
use simplebash_pos_backend::clients::document_server::DocumentServerClient;

async fn spawn(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind mock server");
    let addr = listener.local_addr().expect("failed to get local addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn succeeds_immediately_when_health_is_up() {
    let app = Router::new().route(
        "/api/health",
        get(|| async { Json(json!({ "success": true, "data": { "status": "ok" } })) }),
    );
    let base_url = spawn(app).await;
    let client = DocumentServerClient::new(base_url, "unused".to_string());

    let result = client.wait_until_ready(5).await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[tokio::test]
async fn retries_until_health_reports_ready() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_for_handler = attempts.clone();
    let app = Router::new().route(
        "/api/health",
        get(move || {
            let attempts = attempts_for_handler.clone();
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                if attempt < 2 {
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                } else {
                    Json(json!({ "success": true, "data": { "status": "ok" } })).into_response()
                }
            }
        }),
    );
    let base_url = spawn(app).await;
    let client = DocumentServerClient::new(base_url, "unused".to_string());

    let result = client.wait_until_ready(5).await;

    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert!(
        attempts.load(Ordering::SeqCst) >= 3,
        "expected at least 3 attempts before succeeding"
    );
}

#[tokio::test]
async fn fails_after_exhausting_retries_against_nothing_listening() {
    // Bind then immediately drop the listener to get a URL with nothing
    // behind it, without depending on a specific unused port being free.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind throwaway listener");
    let addr = listener.local_addr().expect("failed to get local addr");
    drop(listener);
    let base_url = format!("http://{addr}");

    let client = DocumentServerClient::new(base_url.clone(), "unused".to_string());

    let result = client.wait_until_ready(2).await;

    let err = result.expect_err("expected Err when nothing is listening");
    assert!(
        err.contains(&base_url),
        "expected error to mention {base_url}, got: {err}"
    );
}
