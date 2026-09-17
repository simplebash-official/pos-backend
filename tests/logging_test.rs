// Request logging (`core::logging::request`): every request gets an
// `X-Request-Id` (echoed when the caller supplied a valid one, minted
// otherwise), and `http/request` + `http/response` lines are written with
// redacted bodies. Captures the JSON log output in-process through a global
// subscriber, so this file must stay a single test (one subscriber per
// test binary).
//
// Needs no live database: `/api/health` and a login body that fails JSON
// extraction never reach a handler that queries storage (same router
// construction as `authorization_test.rs`).

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use simplebash_pos_backend::{app, app::AppState, clients, core::config::Config};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn build_test_app() -> axum::Router {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("invalid configuration for test run");
    // Never connected: the requests below are answered before any query.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .expect("in-memory sqlite pool");
    let db = clients::db::Db::Sqlite(pool);
    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let reports_engine =
        Arc::new(simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db.clone()));
    app::build_router(AppState {
        config,
        db,
        document_server,
        reports_engine,
    })
}

fn log_lines(captured: &Captured) -> Vec<serde_json::Value> {
    String::from_utf8(captured.0.lock().unwrap().clone())
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn span_request_id(line: &serde_json::Value) -> Option<&str> {
    line["spans"]
        .as_array()?
        .iter()
        .find_map(|s| s["request_id"].as_str())
}

#[tokio::test]
async fn requests_get_ids_and_redacted_body_logs() {
    // SAFETY: set before any thread reads the logging settings (first use is
    // `settings()` below), in a test binary with a single test.
    unsafe {
        std::env::set_var("LOG_HTTP_BODIES", "true");
        std::env::set_var("LOG_FORMAT", "json");
    }
    let captured = Captured::default();
    let writer = captured.clone();
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(true)
        .with_writer(move || writer.clone())
        .with_max_level(tracing::Level::INFO)
        .init();

    let router = build_test_app().await;

    // 1. A caller-supplied id is honoured and echoed.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("x-request-id", "req_frontend-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-request-id"], "req_frontend-123");

    // 2. No id → one is minted; an invalid id is replaced, not trusted.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("x-request-id", "not a valid id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let minted = response.headers()["x-request-id"].to_str().unwrap();
    assert!(minted.starts_with("req_"), "minted id: {minted}");

    // 3. A login body is logged with the password masked; the 4xx is a warn.
    let body = r#"{"email":42,"password":"hunter2"}"#;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .header("content-length", body.len())
                .header("x-request-id", "req_login-1")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.status().is_client_error());

    let lines = log_lines(&captured);
    let health_response = lines
        .iter()
        .find(|l| l["event"] == "response" && span_request_id(l) == Some("req_frontend-123"))
        .expect("http/response line for the health request");
    assert_eq!(health_response["category"], "http");
    assert_eq!(health_response["status"], 200);
    assert!(
        health_response["body"]
            .as_str()
            .is_some_and(|b| b.contains("\"status\":\"ok\"")),
        "response body logged: {health_response}"
    );

    let login_request = lines
        .iter()
        .find(|l| l["event"] == "request" && span_request_id(l) == Some("req_login-1"))
        .expect("http/request line for login");
    let logged_body = login_request["body"].as_str().unwrap();
    assert!(logged_body.contains("[REDACTED]"), "{logged_body}");
    assert!(!logged_body.contains("hunter2"), "{logged_body}");

    let login_response = lines
        .iter()
        .find(|l| l["event"] == "response" && span_request_id(l) == Some("req_login-1"))
        .expect("http/response line for login");
    assert_eq!(login_response["level"], "WARN");

    let serialized = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(!serialized.contains("hunter2"), "password leaked into logs");
}
