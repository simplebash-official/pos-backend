//! Supplier crud tests.

use axum::http::StatusCode;

use serde_json::json;
use simplebash_pos_backend::core::config::Config;
use simplebash_pos_backend::core::constants::roles;

use simplebash_pos_backend::domain::users::Role;

use crate::common;
use crate::helpers::{
    admin_token, create_product, create_supplier, sample_supplier_payload, send, send_authed,
};
use uuid::Uuid;

/// A Manager's real token: may register a supplier, nothing else here.
fn manager_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Manager),
        roles::default_permissions(Role::Manager),
    )
}

// ============================================================================
// Supplier CRUD
// ============================================================================

#[tokio::test]
async fn create_supplier_requires_auth() {
    let app = common::spawn_app().await;
    let (status, _) = send(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(sample_supplier_payload()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn suppliers_endpoints_require_admin_role() {
    let app = common::spawn_app().await;

    let (status, body) = send(&app.router, "GET", "/api/suppliers", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let non_admin = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = send_authed(&app.router, "GET", "/api/suppliers", None, &non_admin).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ADMIN_REQUIRED");
}

#[tokio::test]
async fn manager_can_register_a_supplier_but_not_read_or_change_one() {
    let app = common::spawn_app().await;
    let manager = manager_token(&app.config);

    let (_, key, created) = create_supplier(&app.router, &manager).await;
    assert!(key.starts_with("sup_"), "{created}");

    let id = created["id"].as_str().expect("the value is a string");
    for (method, uri, body) in [
        ("GET", "/api/suppliers".to_string(), None),
        ("GET", format!("/api/suppliers/{id}"), None),
        (
            "PATCH",
            format!("/api/suppliers/{id}"),
            Some(json!({ "notes": "x" })),
        ),
        ("DELETE", format!("/api/suppliers/{id}"), None),
    ] {
        let (status, body) = send_authed(&app.router, method, &uri, body, &manager).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {body}");
        assert_eq!(body["code"], "ADMIN_REQUIRED", "{method} {uri}");
    }
}

#[tokio::test]
async fn staff_cannot_register_a_supplier() {
    let app = common::spawn_app().await;
    let staff = common::mint_token(
        &app.config,
        Some(Role::Staff),
        roles::default_permissions(Role::Staff),
    );
    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(sample_supplier_payload()),
        &staff,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "PERMISSION_DENIED");
}

#[test]
fn only_admin_and_manager_hold_suppliers_create() {
    use simplebash_pos_backend::core::constants::permissions::SUPPLIERS_CREATE;
    assert!(roles::default_permissions(Role::Admin).contains(&SUPPLIERS_CREATE));
    assert!(roles::default_permissions(Role::Manager).contains(&SUPPLIERS_CREATE));
    assert!(!roles::default_permissions(Role::Staff).contains(&SUPPLIERS_CREATE));
}

#[tokio::test]
async fn create_supplier_validates_required_fields() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(json!({
            "name": "A",
            "contactPerson": "",
            "primaryPhone": "",
            "address": "",
            "suppliedCategories": [],
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn create_supplier_validates_email_format() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let mut payload = sample_supplier_payload();
    payload["email"] = json!("not-an-email");
    let (status, body) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

#[tokio::test]
async fn create_supplier_allows_empty_email_and_secondary_phone() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let mut payload = sample_supplier_payload();
    payload["email"] = json!("");
    payload["secondaryPhone"] = json!("");
    payload["notes"] = json!("");
    let (status, body) =
        send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(
        body["data"].get("email").is_none() || body["data"]["email"].is_null(),
        "empty email string should be stored as None/null: {body}"
    );
    assert!(
        body["data"].get("secondaryPhone").is_none() || body["data"]["secondaryPhone"].is_null(),
        "empty secondary phone string should be stored as None/null: {body}"
    );
    assert!(
        body["data"].get("notes").is_none() || body["data"]["notes"].is_null(),
        "empty notes string should be stored as None/null: {body}"
    );
}

#[tokio::test]
async fn create_and_get_supplier() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (id, _key, data) = create_supplier(&app.router, &token).await;
    assert_eq!(data["primaryPhone"], "077 123 4567");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["id"], id);
}

#[tokio::test]
async fn list_suppliers_filters_by_search_and_category() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let unique = Uuid::new_v4().simple().to_string();
    let mut payload = sample_supplier_payload();
    payload["name"] = json!(format!("FindableSupplier{unique}"));
    payload["suppliedCategories"] = json!([format!("UniqueTag{unique}")]);
    send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers?search=FindableSupplier{unique}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let suppliers = body["data"]["suppliers"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(suppliers.len(), 1, "{body}");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers?category=UniqueTag{unique}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let suppliers = body["data"]["suppliers"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(suppliers.len(), 1, "{body}");
}

#[tokio::test]
async fn put_replaces_supplier_clearing_omitted_optional_fields() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _key, _) = create_supplier(&app.router, &token).await;

    let replace_payload = json!({
        "name": "Replaced Supplier",
        "contactPerson": "New Contact",
        "primaryPhone": "011 999 8888",
        "address": "456 New Street",
        "suppliedCategories": ["New Category"],
    });
    let (status, body) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/suppliers/{id}"),
        Some(replace_payload),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["name"], "Replaced Supplier");
    assert!(
        body["data"].get("email").is_none(),
        "email should be cleared by PUT: {body}"
    );
}

#[tokio::test]
async fn patch_partial_updates_supplier_leaving_other_fields_unchanged() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _key, original) = create_supplier(&app.router, &token).await;

    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/suppliers/{id}"),
        Some(json!({ "notes": "Updated notes" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["notes"], "Updated notes");
    assert_eq!(body["data"]["primaryPhone"], original["primaryPhone"]);
    assert_eq!(body["data"]["email"], original["email"]);

    // Test patching with empty string clears the optional email
    let (status, body) = send_authed(
        &app.router,
        "PATCH",
        &format!("/api/suppliers/{id}"),
        Some(json!({ "email": "" })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["data"].get("email").is_none() || body["data"]["email"].is_null(),
        "patch with empty email string should clear the field: {body}"
    );
}

#[tokio::test]
async fn delete_supplier_requires_auth_and_removes_it() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id, _key, _) = create_supplier(&app.router, &token).await;

    let (status, _) = send(&app.router, "DELETE", &format!("/api/suppliers/{id}"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send_authed(
        &app.router,
        "DELETE",
        &format!("/api/suppliers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_authed(
        &app.router,
        "GET",
        &format!("/api/suppliers/{id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn batch_delete_suppliers_skips_invalid_ids() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (id_one, _, _) = create_supplier(&app.router, &token).await;
    let (id_two, _, _) = create_supplier(&app.router, &token).await;

    let (status, body) = send_authed(
        &app.router,
        "DELETE",
        "/api/suppliers/batch",
        Some(json!({ "ids": [id_one, id_two, "not-an-object-id"] })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["deletedCount"], 2);
}

#[tokio::test]
async fn supplier_categories_endpoint_returns_distinct_tags() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let unique = Uuid::new_v4().simple().to_string();
    let mut payload = sample_supplier_payload();
    payload["suppliedCategories"] = json!([format!("Tag{unique}")]);
    send_authed(&app.router, "POST", "/api/suppliers", Some(payload), &token).await;

    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/suppliers/categories",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let categories = body["data"]["categories"]
        .as_array()
        .expect("the value is an array");
    assert!(
        categories
            .iter()
            .any(|c| c == &json!(format!("Tag{unique}"))),
        "{body}"
    );
}

// ============================================================================
// Supplier-product links
// ============================================================================

#[tokio::test]
async fn upsert_link_creates_then_updates_in_place() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "costPriceCents": 5000,
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["costPriceCents"], 5000);

    let (status, body) = send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_key,
            "costPriceCents": 6000,
            "notes": "Updated",
        })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["costPriceCents"], 6000);
    assert_eq!(body["data"]["notes"], "Updated");
    // Updating in place must bump the sync `version` rather than re-create the
    // link, and must not leave a `deleted_at` tombstone behind (a present-but-
    // null one would hide the link from every `$exists: false` read below).
    assert_eq!(body["data"]["version"], 2, "{body}");
    assert!(body["data"]["deletedAt"].is_null(), "{body}");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/supplier-products?supplierKey={supplier_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let links = body["data"]["links"]
        .as_array()
        .expect("the value is an array");
    assert_eq!(
        links.len(),
        1,
        "upsert must not create a duplicate link: {body}"
    );
}

#[tokio::test]
async fn supplier_products_endpoints_require_admin_role() {
    let app = common::spawn_app().await;

    let (status, body) = send(&app.router, "GET", "/api/supplier-products", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    let non_admin = common::mint_token(&app.config, Some(Role::Staff), &[]);
    let (status, body) = send_authed(
        &app.router,
        "GET",
        "/api/supplier-products",
        None,
        &non_admin,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "ADMIN_REQUIRED");
}

/// `supplierKey`/`productKey` are optional: an unfiltered `GET` returns the
/// whole collection paginated, which is what a sync client pulls on a first
/// full download. (Assertions stay on the pagination envelope rather than on
/// specific rows — the test database is shared and never cleaned, so page 1 of
/// a `created_at`-ascending listing holds whatever the oldest links happen to
/// be.)
#[tokio::test]
async fn list_links_without_keys_returns_paginated_collection() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({ "supplierKey": supplier_key, "productKey": product_key })),
        &token,
    )
    .await;

    let (status, body) =
        send_authed(&app.router, "GET", "/api/supplier-products", None, &token).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let links = body["data"]["links"]
        .as_array()
        .expect("the value is an array");
    assert!(links.len() <= 20, "default page size is 20: {body}");
    assert!(
        links
            .iter()
            .all(|link| link["supplierKey"].is_string() && link["productKey"].is_string()),
        "{body}"
    );
    assert_eq!(body["data"]["pagination"]["page"], 1, "{body}");
    assert_eq!(body["data"]["pagination"]["limit"], 20, "{body}");
    assert!(
        body["data"]["pagination"]["total"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 1,
        "the link created above must be counted: {body}"
    );
}

#[tokio::test]
async fn unlink_removes_the_link() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_id, product_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({ "supplierKey": supplier_key, "productKey": product_key })),
        &token,
    )
    .await;

    let unlink_uri = format!("/api/supplier-products/{supplier_key}/{product_key}");
    let (status, _) = send_authed(&app.router, "DELETE", &unlink_uri, None, &token).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_authed(&app.router, "DELETE", &unlink_uri, None, &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bulk_replace_preserves_existing_link_metadata() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);
    let (_supplier_id, supplier_key, _) = create_supplier(&app.router, &token).await;
    let (_product_one_id, product_one_key) = create_product(&app, 10).await;
    let (_product_two_id, product_two_key) = create_product(&app, 10).await;
    let (_product_three_id, product_three_key) = create_product(&app, 10).await;

    send_authed(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(json!({
            "supplierKey": supplier_key,
            "productKey": product_one_key,
            "costPriceCents": 4000,
            "notes": "Keep me",
        })),
        &token,
    )
    .await;

    let (status, body) = send_authed(
        &app.router,
        "PUT",
        &format!("/api/supplier-products/bulk/{supplier_key}"),
        Some(json!({ "productKeys": [product_one_key, product_two_key] })),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let links = body["data"].as_array().expect("the value is an array");
    assert_eq!(links.len(), 2);
    let kept = links
        .iter()
        .find(|link| link["productKey"] == json!(product_one_key))
        .expect("retained link missing");
    assert_eq!(kept["costPriceCents"], 4000);
    assert_eq!(kept["notes"], "Keep me");

    let (status, body) = send_authed(
        &app.router,
        "GET",
        &format!("/api/supplier-products?supplierKey={supplier_key}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let links = body["data"]["links"]
        .as_array()
        .expect("the value is an array");
    assert!(
        !links
            .iter()
            .any(|link| link["productKey"] == json!(product_three_key)),
        "unlisted product must not be linked: {body}"
    );
}
