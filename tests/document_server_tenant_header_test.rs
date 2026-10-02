// `clients::document_server::DocumentServerClient` forwards `X-Tenant-Key`
// (via the shared `forwarded_headers()`) whenever a tenant is in ambient
// scope (`TENANT_MODE=multi`, inside `core::tenancy::with_tenant`), and omits
// it entirely otherwise — exactly document-server's own `TenantKey::Single`
// default, i.e. single-shop/desktop behaviour is unchanged. A tiny standalone
// capturing mock (not the shared `tests/common` one, which many other tests
// rely on unmodified) is enough: this only needs to see request headers, not
// exercise a full render.

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::Path,
    http::HeaderMap,
    routing::{get, post},
};
use serde_json::json;
use simplebash_pos_backend::{clients::document_server::DocumentServerClient, core::tenancy};

/// Starts a mock that records every request's headers and answers just
/// enough to satisfy `DocumentServerClient::render` (one template on
/// `GET /api/templates`, a 200 PDF-shaped body on the render POST).
async fn start_capturing_mock() -> (String, Arc<Mutex<Vec<HeaderMap>>>) {
    let captured: Arc<Mutex<Vec<HeaderMap>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_for_templates = captured.clone();
    let captured_for_render = captured.clone();

    let router = Router::new()
        .route(
            "/api/templates",
            get(move |headers: HeaderMap| {
                captured_for_templates.lock().unwrap().push(headers);
                async move {
                    Json(json!({
                        "success": true,
                        "data": { "templates": [
                            { "key": "tpl_receipt", "name": "doc_temp_x", "description": "Thermal Receipt",
                              "dataSchema": { "type": "object", "additionalProperties": true } }
                        ] }
                    }))
                }
            }),
        )
        .route(
            "/api/render/{template_key}",
            post(
                move |Path(_template_key): Path<String>, headers: HeaderMap, _body: axum::body::Bytes| {
                    captured_for_render.lock().unwrap().push(headers);
                    async move { axum::response::Response::new(axum::body::Body::from(b"%PDF-fake".to_vec())) }
                },
            ),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    (format!("http://{addr}"), captured)
}

#[tokio::test]
async fn multi_tenant_scope_sends_the_tenant_header_single_shop_omits_it() {
    let (base_url, captured) = start_capturing_mock().await;
    let client = DocumentServerClient::new(base_url, "unused-in-this-mock".to_string());

    // Single-shop / desktop: no ambient tenant, so no header at all.
    client
        .render("thermal-receipt", json!({ "invoiceNumber": "INV-1" }))
        .await
        .expect("render against the mock should succeed");

    // Multi-tenant: the call happens inside `with_tenant`, so the client
    // must forward the exact tenant id as `X-Tenant-Key`.
    let tenant = tenancy::Tenant::id("tnt_test_header_check").unwrap();
    tenancy::with_tenant(tenant, async {
        client
            .render("thermal-receipt", json!({ "invoiceNumber": "INV-2" }))
            .await
            .expect("render against the mock should succeed")
    })
    .await;

    let seen = captured.lock().unwrap().clone();
    // Two requests per render (GET /api/templates on first cache miss is
    // shared across both calls since the client caches by description, so
    // only the second render's POST necessarily re-hits the network) — find
    // the render POSTs specifically by matching on which one carries no vs.
    // some tenant header, rather than assuming a fixed count/order.
    let without_header = seen
        .iter()
        .filter(|h| h.get("X-Tenant-Key").is_none())
        .count();
    let with_correct_header = seen
        .iter()
        .filter(|h| {
            h.get("X-Tenant-Key")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v == "tnt_test_header_check")
        })
        .count();

    assert!(
        without_header >= 1,
        "the single-shop call must omit X-Tenant-Key: {seen:?}"
    );
    assert!(
        with_correct_header >= 1,
        "the multi-tenant call must send X-Tenant-Key: tnt_test_header_check: {seen:?}"
    );
}
