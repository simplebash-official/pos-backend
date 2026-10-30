mod common;

use axum::body::Body;

use axum::http::{Request, StatusCode};
use simplebash_pos_backend::app::AppState;
use simplebash_pos_backend::clients::db::Db;
use simplebash_pos_backend::{app, clients};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn portal_renders_at_root_and_api() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .expect("in-memory sqlite pool");
    let db = Db::Sqlite(pool);

    let config = common::test_config(&[]);

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
