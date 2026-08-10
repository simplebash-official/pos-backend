mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use bson::DateTime as BsonDateTime;
use chrono::Utc;
use jana2u_pos_backend::{
    core::{constants::prefixes, id::generate_id},
    domain::{
        inventory::Product, sequences::SequenceReservationResponse, sync::SyncChangesResponse,
        users::Role,
    },
    modules::{
        inventory::model::{CategoryDocument, ProductDocument, SubcategoryDocument},
        sync::cursor::encode_cursor,
    },
};
use tower::ServiceExt;
use uuid::Uuid;

async fn send_authed(
    router: &axum::Router,
    token: &str,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    extra_headers: Vec<(&str, &str)>,
) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"));

    for (k, v) in extra_headers {
        builder = builder.header(k, v);
    }

    let request = if let Some(json_body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&json_body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let json: serde_json::Value = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null)
    };

    (status, headers, json)
}

#[tokio::test]
async fn health_endpoint_returns_no_store_and_server_time() {
    let app = common::spawn_app().await;

    let request = Request::builder()
        .uri("/api/health")
        .body(Body::empty())
        .unwrap();

    let response = app.router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let cc = response
        .headers()
        .get(header::CACHE_CONTROL)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(cc, "no-store");

    let server_time = response.headers().get("x-server-time");
    assert!(server_time.is_some(), "missing x-server-time header");
    let st_str = server_time.unwrap().to_str().unwrap();
    assert!(
        chrono::DateTime::parse_from_rfc3339(st_str).is_ok(),
        "x-server-time must be RFC3339"
    );
}

#[tokio::test]
async fn sequence_reservation_allocates_sequential_blocks() {
    let app = common::spawn_app().await;
    let token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    let (status, _, json1) = send_authed(
        &app.router,
        &token,
        "POST",
        "/api/sequences/invoice/reserve",
        Some(serde_json::json!({ "blockSize": 10 })),
        vec![],
    )
    .await;

    assert_eq!(status, StatusCode::OK, "first reservation: {json1}");
    let res1: SequenceReservationResponse = serde_json::from_value(json1["data"].clone()).unwrap();
    assert_eq!(res1.name, "invoice");
    assert_eq!(res1.prefix, "INV-");
    assert_eq!(res1.padding, 6);
    assert_eq!(res1.end - res1.start + 1, 10);

    let (status, _, json2) = send_authed(
        &app.router,
        &token,
        "POST",
        "/api/sequences/invoice/reserve",
        Some(serde_json::json!({ "blockSize": 5 })),
        vec![],
    )
    .await;

    assert_eq!(status, StatusCode::OK, "second reservation: {json2}");
    let res2: SequenceReservationResponse = serde_json::from_value(json2["data"].clone()).unwrap();
    assert_eq!(res2.start, res1.end + 1);
    assert_eq!(res2.end, res1.end + 5);
    assert_eq!(res2.end - res2.start + 1, 5);
}

#[tokio::test]
async fn negative_stock_rejection_returns_409_insufficient_stock() {
    let app = common::spawn_app().await;
    let token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    // Seed category and subcategory
    let cat_key = generate_id("cat");
    let subcat_key = generate_id("subcat");
    let now = BsonDateTime::now();
    app.db
        .collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: cat_key.clone(),
            name: format!("Stock Test Cat {}", Uuid::new_v4()),
            icon: "Box".to_string(),
            color: "blue".to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .unwrap();

    app.db
        .collection::<SubcategoryDocument>("subcategories")
        .insert_one(SubcategoryDocument {
            id: None,
            key: subcat_key.clone(),
            category_key: cat_key.clone(),
            name: "Stock Subcat".to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .unwrap();

    // Create product with stock 5
    let prod_doc = ProductDocument {
        id: None,
        key: generate_id(prefixes::PRODUCT),
        sku: format!("STK-{}", Uuid::new_v4().simple()),
        barcode: None,
        barcode_source: None,
        name: "Stock Test Product".to_string(),
        category_key: cat_key,
        subcategory_key: subcat_key,
        cost_price_cents: 1000,
        selling_price_cents: 1500,
        stock_quantity: 5,
        min_stock_threshold: 2,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        updated_by_device: None,
    };
    let insert_res = app
        .db
        .collection::<ProductDocument>("products")
        .insert_one(prod_doc)
        .await
        .unwrap();
    let prod_id = insert_res.inserted_id.as_object_id().unwrap().to_hex();

    // Adjust by -10 (which would yield -5 stock)
    let (status, _, json) = send_authed(
        &app.router,
        &token,
        "PATCH",
        &format!("/api/inventory/products/{prod_id}/stock"),
        Some(serde_json::json!({ "delta": -10, "note": "oversell test" })),
        vec![],
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "response: {json}");
    assert_eq!(json["code"], "INSUFFICIENT_STOCK");
    assert_eq!(json["details"]["available"], 5);
    assert_eq!(json["details"]["requested"], 10);
    assert_eq!(json["details"]["productId"], prod_id);
}

#[tokio::test]
async fn optimistic_concurrency_product_version_conflict() {
    let app = common::spawn_app().await;
    let token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    let cat_key = generate_id("cat");
    let subcat_key = generate_id("subcat");
    let now = BsonDateTime::now();
    app.db
        .collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: cat_key.clone(),
            name: format!("Opt Test Cat {}", Uuid::new_v4()),
            icon: "Box".to_string(),
            color: "blue".to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .unwrap();

    app.db
        .collection::<SubcategoryDocument>("subcategories")
        .insert_one(SubcategoryDocument {
            id: None,
            key: subcat_key.clone(),
            category_key: cat_key.clone(),
            name: "Opt Subcat".to_string(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .unwrap();

    let prod_doc = ProductDocument {
        id: None,
        key: generate_id(prefixes::PRODUCT),
        sku: format!("OPT-{}", Uuid::new_v4().simple()),
        barcode: None,
        barcode_source: None,
        name: "Opt Concurrency Product".to_string(),
        category_key: cat_key,
        subcategory_key: subcat_key,
        cost_price_cents: 1000,
        selling_price_cents: 2000,
        stock_quantity: 10,
        min_stock_threshold: 2,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        updated_by_device: Some("pos-terminal-1".to_string()),
    };
    let insert_res = app
        .db
        .collection::<ProductDocument>("products")
        .insert_one(prod_doc)
        .await
        .unwrap();
    let prod_id = insert_res.inserted_id.as_object_id().unwrap().to_hex();

    // 1. PUT with wrong If-Match: 2 (expected 1)
    let (status, _, json) = send_authed(
        &app.router,
        &token,
        "PUT",
        &format!("/api/inventory/products/{prod_id}"),
        Some(serde_json::json!({
            "name": "Updated Product Name",
            "sellingPriceCents": 2500,
            "costPriceCents": 1200,
            "stockQuantity": 10,
            "minStockThreshold": 2
        })),
        vec![("if-match", "2"), ("x-device-id", "pos-terminal-2")],
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "should conflict: {json}");
    assert_eq!(json["code"], "VERSION_CONFLICT");
    assert_eq!(json["details"]["expectedVersion"], 2);
    assert_eq!(json["details"]["serverVersion"], 1);
    assert_eq!(json["details"]["updatedByDevice"], "pos-terminal-1");

    // 2. PUT with matching If-Match: 1 (should succeed and increment version to 2)
    let (status, _, json_ok) = send_authed(
        &app.router,
        &token,
        "PUT",
        &format!("/api/inventory/products/{prod_id}"),
        Some(serde_json::json!({
            "name": "Updated Product Name",
            "sellingPriceCents": 2500,
            "costPriceCents": 1200,
            "stockQuantity": 10,
            "minStockThreshold": 2
        })),
        vec![("if-match", "1"), ("x-device-id", "pos-terminal-2")],
    )
    .await;

    assert_eq!(status, StatusCode::OK, "should succeed: {json_ok}");
    let updated_product: Product = serde_json::from_value(json_ok["data"].clone()).unwrap();
    assert_eq!(updated_product.version, 2);
    assert_eq!(updated_product.name, "Updated Product Name");
    assert_eq!(
        updated_product.updated_by_device,
        Some("pos-terminal-2".to_string())
    );
}

#[tokio::test]
async fn sync_changes_cursor_and_tombstones() {
    let app = common::spawn_app().await;
    let token = common::mint_token(&app.config, Some(Role::Admin), &[]);

    // 1. Query with 100-day old cursor (older than 90 days) -> returns 400 CURSOR_INVALID
    let old_time = Utc::now() - chrono::Duration::days(100);
    let expired_cursor = encode_cursor(old_time, "prod_test");

    let (status, _, json_err) = send_authed(
        &app.router,
        &token,
        "GET",
        &format!("/api/sync/changes?since={expired_cursor}&resources=products"),
        None,
        vec![],
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expired cursor: {json_err}"
    );
    assert_eq!(json_err["code"], "CURSOR_INVALID");

    // 2. Initial sync without cursor -> returns full: true
    let (status, _, json_full) = send_authed(
        &app.router,
        &token,
        "GET",
        "/api/sync/changes",
        None,
        vec![],
    )
    .await;

    assert_eq!(status, StatusCode::OK, "full sync: {json_full}");
    let sync_resp: SyncChangesResponse = serde_json::from_value(json_full["data"].clone()).unwrap();
    assert!(sync_resp.changes.contains_key("products"));
    let prod_changes = &sync_resp.changes["products"];
    assert_eq!(prod_changes["full"], true);
}
