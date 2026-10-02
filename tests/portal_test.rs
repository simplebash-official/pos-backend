mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use simplebash_pos_backend::{
    app::{self, AppState},
    clients::{self, db::Db},
    core::config::{Config, DatabaseType, TenantMode},
};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn portal_renders_at_root_and_api() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .expect("in-memory sqlite pool");
    let db = Db::Sqlite(pool);

    let config = Config {
        database_type: DatabaseType::Sqlite,
        database_url: "sqlite::memory:".to_string(),
        mongodb_uri: String::new(),
        mongodb_db_name: String::new(),
        jwt_secret: "test-jwt-secret-key-that-is-at-least-32-characters-long".to_string(),
        port: 8080,
        bind_addr: "127.0.0.1".to_string(),
        jwt_expiry_hours: 12,
        document_server_url: "http://127.0.0.1:8090".to_string(),
        document_server_api_key: "test-key".to_string(),
        generated_documents_dir: "generated_documents".to_string(),
        return_window_days: 30,
        auto_seed: false,
        tenant_mode: TenantMode::Single,
        cors_allowed_origins: vec![],
        identity_jwks_url: None,
        identity_issuer: None,
        identity_tenant_id: None,
        provision_secret: None,
        app_env: "development".to_string(),
    };

    let document_server = clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    );
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db.clone()),
    );

    let state = AppState {
        config: Arc::new(config),
        db,
        document_server: Arc::new(document_server),
        reports_engine,
    };
    let app = app::build_router(state);

    // 1. Test GET /
    let res = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("text/html"));

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("SimpleBash POS Core API"));
    assert!(body_str.contains("telemetry-signal"));
    assert!(body_str.contains("uptime-ticker"));
    assert!(body_str.contains("Development"));

    // 2. Test GET /api
    let res_api = app
        .clone()
        .oneshot(Request::builder().uri("/api").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res_api.status(), StatusCode::OK);

    // 3. Test GET /api/health returns live telemetry
    let res_health = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res_health.status(), StatusCode::OK);
    let health_bytes = axum::body::to_bytes(res_health.into_body(), usize::MAX)
        .await
        .unwrap();
    let health_json: serde_json::Value = serde_json::from_slice(&health_bytes).unwrap();
    assert_eq!(health_json["data"]["status"], "ok");
    assert!(health_json["data"]["uptime_seconds"].is_number());
    assert_eq!(health_json["data"]["environment"], "development");
}
