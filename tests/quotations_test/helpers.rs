//! Shared setup for the `quotations_test` tests.

use crate::common;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use simplebash_pos_backend::app::AppState;
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::roles;
use simplebash_pos_backend::core::logging::SqlLogging;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::{app, clients};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// Quotations: Admin/Manager-only access, server-side totals, validation,
// numbering, list filters, cancel, PDF printing, and "no side effects on
// stock or invoices". The behaviour scenarios run against in-memory-style
// SQLite (always); a smaller smoke test runs the same helpers against the
// real Mongo `spawn_app()` so the Mongo repository branch is exercised too.

// ============================================================================
// SQLite harness (same construction as `sqlite_integration_test.rs`)
// ============================================================================

pub(crate) struct Ctx {
    pub(crate) router: axum::Router,
    pub(crate) config: Arc<Config>,
    pub(crate) db_url: String,
    pub(crate) db_path: std::path::PathBuf,
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_file(self.db_path.with_extension("db-shm"));
        let _ = std::fs::remove_file(self.db_path.with_extension("db-wal"));
    }
}

pub(crate) async fn setup() -> Ctx {
    dotenvy::dotenv().ok();
    let db_path = std::env::temp_dir().join(format!("pos_quo_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = clients::sqlite::connect(&db_url, SqlLogging::Off)
        .await
        .expect("failed to initialize SQLite test database");

    let config = Arc::new(common::test_config(&[
        ("DATABASE_URL", &db_url),
        (
            "DOCUMENT_SERVER_URL",
            &common::start_mock_document_server().await,
        ),
        (
            "GENERATED_DOCUMENTS_DIR",
            &std::env::temp_dir().display().to_string(),
        ),
    ]));
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let db = Db::Sqlite(pool);
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db.clone()),
    );
    let router = app::build_router(AppState {
        config: config.clone(),
        db,
        document_server,
        reports_engine,
    });
    Ctx {
        router,
        config,
        db_url,
        db_path,
    }
}

// ============================================================================
// Request helpers (shared by the SQLite and Mongo runs)
// ============================================================================

pub(crate) async fn request(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).expect("the value serializes to JSON")),
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(builder.body(body).expect("the value serializes to JSON"))
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds")
        .to_vec();
    (status, headers, bytes)
}

pub(crate) async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let (status, _, bytes) = request(router, method, uri, body, Some(token)).await;
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

pub(crate) fn token(config: &Config, role: Role) -> String {
    common::mint_token(config, Some(role), roles::default_permissions(role))
}

pub(crate) fn date_in(days: i64) -> String {
    (Utc::now() + Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

/// A valid request: two ad-hoc lines (22,000 gross, one line discount) and a
/// 10% order discount, so every total below is checkable by hand.
pub(crate) fn valid_body() -> Value {
    json!({
        "customer": { "customerName": "Nimal Perera", "customerPhone": "0771234567",
                      "customerAddress": "12 Temple Road, Kandy" },
        "items": [
            { "name": "Screen replacement", "unitPriceCents": 10000, "quantity": 2, "discountCents": 500 },
            { "name": "Tempered glass", "unitPriceCents": 2500, "quantity": 1 }
        ],
        "pricingAdjustments": { "discountType": "percentage", "discountValue": 10.0 },
        "validUntil": date_in(14),
        "notes": "Price excludes courier.",
        "terms": "50% advance to start work.",
        "preparedBy": "Kamal"
    })
}

pub(crate) async fn create(router: &axum::Router, tok: &str, body: Value) -> (StatusCode, Value) {
    call(router, "POST", "/api/quotations", Some(body), tok).await
}
