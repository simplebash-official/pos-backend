// Builds the real router with a Mongo client that's never pinged (needs
// `MONGODB_URI` to be a syntactically valid connection string via `.env`,
// but no live Mongo) — for asserting routing/OpenAPI-doc wiring, not data.
// See CLAUDE.md's "three integration test files" breakdown for how this
// compares to `scenarios_test.rs`/`response_format_test.rs`.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use jana2u_pos_backend::{app, app::AppState, core::config::Config};
use mongodb::Client;
use mongodb::options::{ClientOptions, ResolverConfig};
use tower::ServiceExt;

async fn build_test_app() -> axum::Router {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("invalid configuration for test run");
    let options = ClientOptions::parse(&config.mongodb_uri)
        .resolver_config(ResolverConfig::cloudflare())
        .await
        .expect("valid mongodb uri");
    let client = Client::with_options(options).expect("client construction");
    let db = client.database(&config.mongodb_db_name);

    let state = AppState {
        config: Arc::new(config),
        db,
    };
    app::build_router(state)
}

#[tokio::test]
async fn openapi_json_lists_all_module_paths() {
    let router = build_test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/api-docs/openapi.json")
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
    let paths = json["paths"].as_object().unwrap();

    for expected in [
        "/api/health",
        "/api/auth",
        "/api/billing",
        "/api/customers",
        "/api/inventory",
        "/api/print-jobs",
        "/api/repairs",
        "/api/reports",
    ] {
        assert!(paths.contains_key(expected), "missing path: {expected}");
    }
}

#[tokio::test]
async fn swagger_ui_is_mounted() {
    let router = build_test_app().await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/docs/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
