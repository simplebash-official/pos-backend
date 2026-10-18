// The shop's custom design packages for desktop apps: `GET /api/document-packages`
// lists the private designs the shop may use (from the document-server's
// per-tenant list) and `GET /api/document-packages/{name}/archive` proxies a
// download. Cloud (multi-tenant) only. Multi-tenant cases run against a
// throwaway Atlas database; the single-shop 404 on SQLite.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use serde_json::Value;
use simplebash_pos_backend::{core::constants::roles, domain::users::Role};
use tower::ServiceExt;

async fn get(router: &axum::Router, uri: &str, token: &str) -> (StatusCode, String, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, content_type, bytes.to_vec())
}

#[tokio::test]
async fn a_shop_lists_and_downloads_its_custom_designs() {
    let app = common::spawn_app_multi_tenant().await;
    let staff = common::mint_token_for_tenant(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
        Some("tnt_shop_a"),
    );

    let (status, _, bytes) = get(&app.router, "/api/document-packages", &staff).await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    // Only the custom (private) design: bundled ones ship with every install.
    assert_eq!(
        body["data"]["packages"],
        serde_json::json!([{
            "name": "doc_temp_ClientInvoiceV2",
            "family": "fam_ClientInvoice00",
            "version": 2,
            "kind": "a4-invoice",
            "title": "Client Invoice",
            "type": "document",
            "sha256": "a".repeat(64),
            "minAppVersion": null
        }])
    );

    let (status, content_type, archive) = get(
        &app.router,
        "/api/document-packages/doc_temp_ClientInvoiceV2/archive",
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/zip");
    assert_eq!(archive, b"PK mock archive of doc_temp_ClientInvoiceV2");

    // The document-server's refusal (not a package this shop may use) is passed on.
    let (status, _, _) = get(
        &app.router,
        "/api/document-packages/doc_temp_nobody/archive",
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    app.db.drop().await.ok();
}

#[tokio::test]
async fn a_single_shop_deployment_has_no_packages_to_offer() {
    let app = common::spawn_app_sqlite().await;
    let token = common::mint_token(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    );
    for uri in [
        "/api/document-packages",
        "/api/document-packages/doc_temp_ClientInvoiceV2/archive",
    ] {
        let (status, _, _) = get(&app.router, uri, &token).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
}
