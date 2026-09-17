// Builds the real router with a Mongo client that's never pinged (needs
// `MONGODB_URI` to be a syntactically valid connection string via `.env`,
// but no live Mongo) — for asserting routing/OpenAPI-doc wiring, not data.
// See CLAUDE.md's "three integration test files" breakdown for how this
// compares to `scenarios_test.rs`/`response_format_test.rs`.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use myrologic_pos_backend::{app, app::AppState, clients, core::config::Config};
use mongodb::Client;
use mongodb::options::{ClientOptions, ResolverConfig};
use tower::ServiceExt;

async fn build_test_app() -> axum::Router {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("invalid configuration for test run");
    let db_handle = match config.database_type {
        myrologic_pos_backend::core::config::DatabaseType::Sqlite => {
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .expect("in-memory sqlite pool");
            myrologic_pos_backend::clients::db::Db::Sqlite(pool)
        }
        myrologic_pos_backend::core::config::DatabaseType::Mongo => {
            let uri = if config.mongodb_uri.is_empty() {
                "mongodb://localhost:27017"
            } else {
                &config.mongodb_uri
            };
            let parse_res = ClientOptions::parse(uri).await;
            let options = match parse_res {
                Ok(opts) => opts,
                Err(_) => match ClientOptions::parse(uri)
                    .resolver_config(ResolverConfig::cloudflare())
                    .await
                {
                    Ok(opts) => opts,
                    Err(_) => ClientOptions::parse(uri)
                        .resolver_config(ResolverConfig::google())
                        .await
                        .expect("valid mongodb uri"),
                },
            };
            let client = Client::with_options(options).expect("client construction");
            let db_name = if config.mongodb_db_name.is_empty() {
                "myrologic_pos_test"
            } else {
                &config.mongodb_db_name
            };
            let db = client.database(db_name);
            myrologic_pos_backend::clients::db::Db::Mongo(db)
        }
    };
    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let reports_engine = Arc::new(
        myrologic_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
    );
    let state = AppState {
        config,
        db: db_handle,
        document_server,
        reports_engine,
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
        "/api/auth/login",
        "/api/auth/me",
        "/api/auth/sessions",
        "/api/billing",
        "/api/customers",
        "/api/inventory",
        "/api/print-jobs",
        "/api/repairs",
        "/api/reports",
        "/api/suppliers",
        "/api/supplier-products",
        "/api/purchases",
        "/api/sequences/{name}/reserve",
        "/api/sync/changes",
        "/api/sync/status",
        "/api/inventory/stock-movements",
        "/api/users",
        "/api/backup/export",
        "/api/backup/import",
    ] {
        assert!(paths.contains_key(expected), "missing path: {expected}");
    }
}

#[tokio::test]
async fn response_body_carries_processing_time_on_success() {
    let router = build_test_app().await;

    let response = router
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
    assert!(
        json["processingTimeMs"].as_u64().is_some(),
        "missing processingTimeMs in body: {json}"
    );
}

#[tokio::test]
async fn response_body_carries_processing_time_on_error() {
    let router = build_test_app().await;

    // Missing bearer token trips `CurrentUser`'s extractor before the
    // handler ever touches Mongo, so this 401 comes back through the normal
    // `AppError` -> `ErrorResponse` path without needing a live database.
    let response = router
        .oneshot(
            Request::builder()
                .uri("/api/auth/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["processingTimeMs"].as_u64().is_some(),
        "missing processingTimeMs in body: {json}"
    );
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
