// Builds the real router with a Mongo client that's never pinged (needs
// `MONGODB_URI` to be a syntactically valid connection string via `.env`,
// but no live Mongo) — for asserting routing/OpenAPI-doc wiring, not data.
// See CLAUDE.md's "three integration test files" breakdown for how this
// compares to `scenarios_test.rs`/`response_format_test.rs`.

mod common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use simplebash_pos_backend::app::AppState;
use simplebash_pos_backend::{app, clients};
use tower::ServiceExt;

async fn build_test_app() -> axum::Router {
    let config = common::test_config(&[]);
    // Never connected: the tests only inspect the routes.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .expect("in-memory sqlite pool");
    let db_handle = simplebash_pos_backend::clients::db::Db::Sqlite(pool);
    let config = Arc::new(config);
    let document_server = Arc::new(clients::document_server::DocumentServerClient::new(
        config.document_server_url.clone(),
        config.document_server_api_key.clone(),
    ));
    let reports_engine = Arc::new(
        simplebash_pos_backend::modules::reports::engine::AnalyticsEngine::new(db_handle.clone()),
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
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("the text is valid JSON");
    let paths = json["paths"].as_object().expect("the value is an object");

    for expected in [
        "/api/health",
        "/api/auth/login",
        "/api/auth/me",
        "/api/auth/sessions",
        "/api/billing",
        "/api/customers",
        "/api/inventory",
        "/api/print-jobs",
        "/api/quotations",
        "/api/repairs",
        "/api/reports",
        "/api/suppliers",
        "/api/supplier-products",
        "/api/purchases",
        "/api/purchase-receipts",
        "/api/purchase-receipts/{key}",
        "/api/sequences/{name}/reserve",
        "/api/sync/changes",
        "/api/sync/status",
        "/api/inventory/stock-movements",
        "/api/users",
        "/api/backup/export",
        "/api/backup/import",
        "/api/settings/shop-profile",
        "/api/settings/document-templates",
        "/api/settings/document-templates/{family}/preview",
        "/api/billing/invoices/{id}/documents/{documentType}/design",
        "/api/billing/credit-notes/{id}/documents/{documentType}/design",
        "/api/quotations/{id}/document/design",
        "/api/document-packages",
        "/api/document-packages/{name}/archive",
        "/api/internal/tenants/{tenant_id}/document-selections",
        "/api/internal/tenants/{tenant_id}/document-selections/reset",
        "/api/labels/templates",
        "/api/labels/templates/{key}",
        "/api/labels/render",
        "/api/labels/preview",
        "/api/inventory/products/{id}/barcode",
        "/api/inventory/serials/by-number/{serialNumber}",
        "/api/calc/warranty",
        "/api/warranties",
        "/api/warranties/lookup",
        "/api/warranties/{key}",
        "/api/warranties/{key}/void",
        "/api/warranties/{key}/extend",
        "/api/warranty-claims",
        "/api/warranty-claims/{key}",
        "/api/warranty-claims/{key}/inspect",
        "/api/warranty-claims/{key}/decide",
        "/api/warranty-claims/{key}/supplier/send",
        "/api/warranty-claims/{key}/supplier/return",
        "/api/warranty-claims/{key}/resolve",
        "/api/warranty-claims/{key}/return",
        "/api/warranty-claims/{key}/paid-repair",
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
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("the text is valid JSON");
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
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("the text is valid JSON");
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
                .expect("the request is well formed"),
        )
        .await
        .expect("the awaited step succeeds");

    assert_eq!(response.status(), StatusCode::OK);
}
