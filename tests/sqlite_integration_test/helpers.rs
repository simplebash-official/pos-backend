//! Shared setup for the `sqlite_integration_test` tests.

use crate::common;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use serde_json::Value;
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

pub(crate) struct SqliteTestContext {
    pub(crate) router: axum::Router,
    pub(crate) config: Arc<Config>,
    pub(crate) db_path: std::path::PathBuf,
}

impl Drop for SqliteTestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let shm = self.db_path.with_extension("db-shm");
        let wal = self.db_path.with_extension("db-wal");
        let _ = std::fs::remove_file(shm);
        let _ = std::fs::remove_file(wal);
    }
}

pub(crate) async fn setup_sqlite_app() -> SqliteTestContext {
    dotenvy::dotenv().ok();
    let db_path = std::env::temp_dir().join(format!("pos_test_{}.db", Uuid::new_v4()));
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = clients::sqlite::connect(&db_url, SqlLogging::Off)
        .await
        .expect("failed to initialize SQLite test database");

    let mock_doc_server_url = common::start_mock_document_server().await;

    let config = common::test_config(&[
        ("DATABASE_URL", &db_url),
        ("DOCUMENT_SERVER_URL", &mock_doc_server_url),
        (
            "GENERATED_DOCUMENTS_DIR",
            &std::env::temp_dir().display().to_string(),
        ),
    ]);

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let db_handle = Db::Sqlite(pool);
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
    );
    let state = AppState {
        config: config.clone(),
        db: db_handle,
        document_server,
        reports_engine,
    };

    SqliteTestContext {
        router: app::build_router(state),
        config,
        db_path,
    }
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
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
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
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json");
    let body_bytes = match body {
        Some(val) => Body::from(serde_json::to_vec(&val).expect("the value serializes to JSON")),
        None => Body::empty(),
    };
    execute(
        router,
        builder
            .body(body_bytes)
            .expect("the value serializes to JSON"),
    )
    .await
}

pub(crate) fn admin_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}
