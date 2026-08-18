// Builds the real router with a Mongo client that's never pinged (needs
// `MONGODB_URI` to be a syntactically valid connection string via `.env`,
// but no live Mongo) — for asserting routing/OpenAPI-doc wiring, not data.
// See CLAUDE.md's "three integration test files" breakdown for how this
// compares to `scenarios_test.rs`/`response_format_test.rs`.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use jana2u_pos_backend::{app, app::AppState, clients, core::config::Config};
use mongodb::Client;
use mongodb::options::{ClientOptions, ResolverConfig};
use tower::ServiceExt;

async fn build_test_app() -> axum::Router {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("invalid configuration for test run");
    let parse_res = ClientOptions::parse(&config.mongodb_uri).await;
    let options = match parse_res {
        Ok(opts) => opts,
        Err(_) => match ClientOptions::parse(&config.mongodb_uri)
            .resolver_config(ResolverConfig::cloudflare())
            .await
        {
            Ok(opts) => opts,
            Err(_) => ClientOptions::parse(&config.mongodb_uri)
                .resolver_config(ResolverConfig::google())
                .await
                .expect("valid mongodb uri"),
        },
    };
    let client = Client::with_options(options).expect("client construction");
    let db = client.database(&config.mongodb_db_name);

    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let state = AppState {
        config,
        db,
        document_server,
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
