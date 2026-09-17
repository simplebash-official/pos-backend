mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use bson::DateTime as BsonDateTime;
use chrono::Utc;
use myrologic_pos_backend::{
    core::{
        config::Config,
        constants::{prefixes, roles},
        id::generate_id,
    },
    domain::{
        billing::{Invoice, InvoiceStatus, PaymentRecord},
        inventory::Product,
        sequences::SequenceReservationResponse,
        sync::SyncChangesResponse,
        users::Role,
    },
    modules::{
        billing::model::{InvoiceDocument, PaymentDocument},
        inventory::model::{CategoryDocument, ProductDocument, SubcategoryDocument},
        sync::cursor::encode_cursor,
    },
};
use tower::ServiceExt;
use uuid::Uuid;

/// Admin token carrying its *real* permission set rather than an empty one,
/// so it satisfies the permission gates on the inventory routes these tests
/// drive (`inventory:read`/`inventory:write`) and not just the role checks.
fn admin_token(config: &Config) -> String {
    common::mint_token(
        config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
    )
}

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
    let token = admin_token(&app.config);

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
    let token = admin_token(&app.config);

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
        is_serialized: false,
        warranty_months: None,
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
    let token = admin_token(&app.config);

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
        is_serialized: false,
        warranty_months: None,
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
    let token = admin_token(&app.config);

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

#[tokio::test]
async fn sync_status_returns_latest_resource_timestamps() {
    let app = common::spawn_app().await;

    // 1. Unauthenticated request -> 401
    let req = Request::builder()
        .uri("/api/sync/status")
        .body(Body::empty())
        .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // 2. Insert Category and Product into MongoDB
    let token = admin_token(&app.config);
    let cat_key = generate_id("cat");
    let now = BsonDateTime::now();

    app.db
        .collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: cat_key.clone(),
            name: format!("Status Cat {}", Uuid::new_v4()),
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

    // 3. Authenticated request -> 200 with a per-resource watermark
    let (status, _, json) =
        send_authed(&app.router, &token, "GET", "/api/sync/status", None, vec![]).await;

    assert_eq!(status, StatusCode::OK, "sync status response: {json}");
    assert_eq!(json["message"], "Sync status retrieved");

    let resources = json["data"]["resources"]
        .as_object()
        .expect("resources must be an object");
    assert!(resources.contains_key("categories"));

    let cat_ts = resources["categories"]["lastUpdatedAt"].as_str().unwrap();
    assert!(
        chrono::DateTime::parse_from_rfc3339(cat_ts).is_ok(),
        "categories timestamp must be valid RFC3339: {cat_ts}"
    );

    // The cursor is the whole reason this endpoint exists for the client:
    // after taking a snapshot it adopts this value so its next pull is a
    // true delta. `/sync/changes` pages oldest-first and cannot supply it.
    let cursor = resources["categories"]["cursor"]
        .as_str()
        .expect("categories must carry a cursor");
    assert!(!cursor.is_empty());

    assert!(
        json["data"]["serverTime"].is_string(),
        "status must carry serverTime for client clock-skew handling"
    );

    // That cursor must be usable, and must be the *newest* one: replaying it
    // immediately returns nothing rather than the whole collection.
    let (status, _, json) = send_authed(
        &app.router,
        &token,
        "GET",
        &format!("/api/sync/changes?resources=categories&cursors=categories={cursor}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "replay response: {json}");
    let changes = &json["data"]["changes"]["categories"];
    assert_eq!(changes["full"], false);
    assert_eq!(changes["unchanged"], true);
    assert_eq!(changes["items"].as_array().unwrap().len(), 0);
}

/// The delta feed and the REST snapshot feed are mirrored into the same
/// local table by the client, so a row that arrives via `/sync/changes` has
/// to be indistinguishable from the same row returned by `GET /products`.
///
/// This previously was not true: the endpoint serialized raw BSON documents,
/// so items came back snake_case, with `{"$oid"}`/`{"$date"}` wrappers, and
/// without the `category`/`subcategory` names the read path resolves — and a
/// category arrived with no `subcategories` array at all, which is what made
/// a delta pull wipe the subcategory tree out of the client's mirror.
#[tokio::test]
async fn sync_changes_items_match_the_rest_dto_shape() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    // The feed pages oldest-first, and a shared test database holds far more
    // than one page of rows. Anchoring to a cursor from just before the
    // inserts below scopes the response to exactly the seeded rows.
    let watermark = encode_cursor(Utc::now() - chrono::Duration::seconds(5), "");

    let cat_key = generate_id(prefixes::CATEGORY);
    let sub_key = generate_id(prefixes::SUBCATEGORY);
    let now = BsonDateTime::now();
    let category_name = format!("Shape Cat {}", Uuid::new_v4());
    let subcategory_name = format!("Shape Sub {}", Uuid::new_v4());

    app.db
        .collection::<CategoryDocument>("categories")
        .insert_one(CategoryDocument {
            id: None,
            key: cat_key.clone(),
            name: category_name.clone(),
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
            key: sub_key.clone(),
            category_key: cat_key.clone(),
            name: subcategory_name.clone(),
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .unwrap();

    let product_key = generate_id(prefixes::PRODUCT);
    app.db
        .collection::<ProductDocument>("products")
        .insert_one(ProductDocument {
            id: None,
            key: product_key.clone(),
            sku: format!("SKU-{}", Uuid::new_v4()),
            barcode: None,
            barcode_source: None,
            name: "Shape Probe Product".to_string(),
            category_key: cat_key.clone(),
            subcategory_key: sub_key.clone(),
            cost_price_cents: 1000,
            selling_price_cents: 1500,
            stock_quantity: 7,
            min_stock_threshold: 2,
            is_serialized: false,
            warranty_months: None,
            version: 1,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            updated_by_device: None,
        })
        .await
        .unwrap();

    let (status, _, json) = send_authed(
        &app.router,
        &token,
        "GET",
        &format!("/api/sync/changes?resources=products,categories&since={watermark}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sync changes: {json}");

    let products = json["data"]["changes"]["products"]["items"]
        .as_array()
        .expect("products items must be an array");
    let product = products
        .iter()
        .find(|item| item["key"] == serde_json::json!(product_key))
        .expect("the seeded product must appear in the delta feed");

    // Deserializing into the very type `GET /products` returns is the
    // strongest available assertion that the two feeds agree.
    let typed: Product = serde_json::from_value(product.clone())
        .expect("a sync item must deserialize as the REST Product DTO");
    assert_eq!(typed.stock_quantity, 7);
    assert_eq!(typed.version, 1);

    // The display names the read path resolves must be present, not the keys.
    assert_eq!(typed.category, category_name);
    assert_eq!(typed.subcategory, subcategory_name);

    // camelCase, and no BSON wrappers leaking through.
    assert!(product["costPriceCents"].is_number());
    assert!(product.get("cost_price_cents").is_none());
    assert!(product["id"].is_string(), "_id must be a hex string");
    assert!(product.get("_id").is_none());
    assert!(
        product["createdAt"].is_string(),
        "dates must be ISO strings, not {{$date}}"
    );

    // A category must carry its subcategories, which live in their own
    // collection and are absent from the raw document.
    let categories = json["data"]["changes"]["categories"]["items"]
        .as_array()
        .expect("categories items must be an array");
    let category = categories
        .iter()
        .find(|item| item["key"] == serde_json::json!(cat_key))
        .expect("the seeded category must appear in the delta feed");

    let subcategories = category["subcategories"]
        .as_array()
        .expect("a synced category must carry its subcategories");
    assert!(
        subcategories
            .iter()
            .any(|s| s["name"] == serde_json::json!(subcategory_name)),
        "subcategories must be resolved into the category: {category}"
    );

    // `subcategories` is folded into categories, never its own resource.
    assert!(
        json["data"]["changes"].get("subcategories").is_none(),
        "subcategories must not be a standalone syncable resource"
    );
}

/// Same DTO-parity guard as `sync_changes_items_match_the_rest_dto_shape`,
/// covering `invoices`/`payments`. Unlike `sync_changes_cursor_and_tombstones`,
/// there is deliberately no tombstone-path counterpart here: invoices and
/// payments are append-only (no `deleted_at` field, no delete route — a
/// cancelled invoice flips `status` rather than being removed), so a delta
/// pull for either resource can never contain a `deleted` entry.
#[tokio::test]
async fn sync_changes_invoices_and_payments_match_the_rest_dto_shape() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let watermark = encode_cursor(Utc::now() - chrono::Duration::seconds(5), "");

    let invoice_key = generate_id(prefixes::INVOICE);
    let now = BsonDateTime::now();
    let invoice_number = format!("INV-{}", Uuid::new_v4());

    app.db
        .collection::<InvoiceDocument>("invoices")
        .insert_one(InvoiceDocument {
            id: None,
            key: invoice_key.clone(),
            invoice_number: invoice_number.clone(),
            customer_key: None,
            customer_name_snapshot: None,
            customer_phone_snapshot: None,
            customer_address_snapshot: None,
            cashier_id: "usr_shape_test".to_string(),
            cashier_name_snapshot: "Shape Test Cashier".to_string(),
            items: vec![],
            subtotal_cents: 5000,
            discount_type: "fixed".to_string(),
            discount_value: 0.0,
            discount_cents: 0,
            total_cents: 5000,
            payment_method: "cash".to_string(),
            split_payments: None,
            is_credit: false,
            amount_received_cents: Some(5000),
            change_due_cents: Some(0),
            due_date: None,
            card_last4: None,
            card_ref: None,
            online_ref: None,
            online_note: None,
            status: InvoiceStatus::Paid,
            notes: None,
            shop_profile_snapshot: serde_json::json!({ "tradingName": "Shape Test Shop" }),
            warranty_terms_snapshot: None,
            document_selection: None,
            voided_at: None,
            voided_by: None,
            voided_reason: None,
            closed_at: None,
            closed_by: None,
            credit_note_count: 0,
            refunded_cents: 0,
            version: 1,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    let payment_key = generate_id(prefixes::PAYMENT);
    app.db
        .collection::<PaymentDocument>("payments")
        .insert_one(PaymentDocument {
            id: None,
            key: payment_key.clone(),
            invoice_key: invoice_key.clone(),
            amount_cents: 5000,
            payment_method: "cash".to_string(),
            notes: None,
            recorded_by_user_id: "usr_shape_test".to_string(),
            recorded_by_name_snapshot: "Shape Test Cashier".to_string(),
            recorded_at: now,
            version: 1,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    let (status, _, json) = send_authed(
        &app.router,
        &token,
        "GET",
        &format!("/api/sync/changes?resources=invoices,payments&since={watermark}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sync changes: {json}");

    let invoices = json["data"]["changes"]["invoices"]["items"]
        .as_array()
        .expect("invoices items must be an array");
    let invoice = invoices
        .iter()
        .find(|item| item["key"] == serde_json::json!(invoice_key))
        .expect("the seeded invoice must appear in the delta feed");

    let typed_invoice: Invoice = serde_json::from_value(invoice.clone())
        .expect("a sync item must deserialize as the REST Invoice DTO");
    assert_eq!(typed_invoice.total_cents, 5000);
    assert_eq!(typed_invoice.status, InvoiceStatus::Paid);
    assert_eq!(typed_invoice.invoice_number, invoice_number);

    // camelCase, and no BSON wrappers leaking through.
    assert!(invoice["totalCents"].is_number());
    assert!(invoice.get("total_cents").is_none());
    assert!(invoice["invoiceNumber"].is_string());
    assert!(invoice.get("invoice_number").is_none());
    assert!(invoice["id"].is_string(), "_id must be a hex string");
    assert!(invoice.get("_id").is_none());
    assert!(
        invoice["createdAt"].is_string(),
        "dates must be ISO strings, not {{$date}}"
    );

    let payments = json["data"]["changes"]["payments"]["items"]
        .as_array()
        .expect("payments items must be an array");
    let payment = payments
        .iter()
        .find(|item| item["key"] == serde_json::json!(payment_key))
        .expect("the seeded payment must appear in the delta feed");

    let typed_payment: PaymentRecord = serde_json::from_value(payment.clone())
        .expect("a sync item must deserialize as the REST PaymentRecord DTO");
    assert_eq!(typed_payment.amount_cents, 5000);
    assert_eq!(typed_payment.invoice_key, invoice_key);

    // camelCase, and no BSON wrappers leaking through.
    assert!(payment["amountCents"].is_number());
    assert!(payment.get("amount_cents").is_none());
    assert!(payment["invoiceKey"].is_string());
    assert!(payment.get("invoice_key").is_none());
    assert!(payment["id"].is_string(), "_id must be a hex string");
    assert!(payment.get("_id").is_none());
}

/// Same DTO-parity guard as `sync_changes_items_match_the_rest_dto_shape`,
/// covering the new `employees` resource — the one resource whose
/// `hydrate_sync_documents` is async and enriches each row with a
/// live-resolved `login` summary (see
/// `modules::employees::service::hydrate_sync_documents`).
#[tokio::test]
async fn sync_changes_employees_match_the_rest_dto_shape() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let watermark = encode_cursor(Utc::now() - chrono::Duration::seconds(5), "");

    let employee_name = format!("Shape Employee {}", Uuid::new_v4());
    let (status, _, created) = send_authed(
        &app.router,
        &token,
        "POST",
        "/api/employees",
        Some(serde_json::json!({
            "name": employee_name,
            "phone": "0771234567",
            "role": "technician",
            "defaultSplitType": "percentage",
            "defaultSplitValue": 20.0,
        })),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let employee_key = created["data"]["key"].as_str().unwrap().to_string();

    let (status, _, json) = send_authed(
        &app.router,
        &token,
        "GET",
        &format!("/api/sync/changes?resources=employees&since={watermark}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sync changes: {json}");

    let items = json["data"]["changes"]["employees"]["items"]
        .as_array()
        .expect("employees items must be an array");
    let item = items
        .iter()
        .find(|item| item["key"] == serde_json::json!(employee_key))
        .expect("the seeded employee must appear in the delta feed");

    let typed: myrologic_pos_backend::domain::employees::Employee =
        serde_json::from_value(item.clone())
            .expect("a sync item must deserialize as the REST Employee DTO");
    assert_eq!(typed.name, employee_name);
    assert!(typed.login.is_none(), "no login was ever linked");

    // camelCase, and no BSON wrappers leaking through.
    assert!(item["defaultSplitValue"].is_number());
    assert!(item.get("default_split_value").is_none());
    assert!(item["id"].is_string(), "_id must be a hex string");
    assert!(item.get("_id").is_none());
}

/// Regression test: linking a login to an employee doesn't change any field
/// on the employee document itself (the link lives on the `users`
/// collection), so without an explicit `updated_at` bump the sync delta
/// feed would never re-deliver that employee row — an offline mirror would
/// show "no login" forever, even after the employee gains one. See
/// `modules::employees::service::touch_by_key`.
#[tokio::test]
async fn sync_changes_redelivers_an_employee_after_a_login_is_linked() {
    let app = common::spawn_app().await;
    let token = admin_token(&app.config);

    let (status, _, created) = send_authed(
        &app.router,
        &token,
        "POST",
        "/api/employees",
        Some(serde_json::json!({
            "name": format!("Touch Regression {}", Uuid::new_v4()),
            "phone": "0771234567",
            "role": "technician",
            "defaultSplitType": "percentage",
            "defaultSplitValue": 20.0,
        })),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let employee_key = created["data"]["key"].as_str().unwrap().to_string();

    // Watermark taken strictly after the employee's own create — only the
    // login-linking touch should be visible in the delta from here on.
    let watermark = encode_cursor(Utc::now() + chrono::Duration::milliseconds(50), "");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (status, _, _) = send_authed(
        &app.router,
        &token,
        "POST",
        "/api/users",
        Some(serde_json::json!({
            "name": "Touch Regression Login",
            "email": format!("touch-regression-{}@example.com", Uuid::new_v4()),
            "password": "Password123!",
            "role": "staff",
            "employeeKey": employee_key,
        })),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _, json) = send_authed(
        &app.router,
        &token,
        "GET",
        &format!("/api/sync/changes?resources=employees&since={watermark}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sync changes: {json}");

    let items = json["data"]["changes"]["employees"]["items"]
        .as_array()
        .expect("employees items must be an array");
    let item = items
        .iter()
        .find(|item| item["key"] == serde_json::json!(employee_key))
        .expect(
            "the employee must reappear in the delta after its login was linked, \
             even though no field on the employee document itself changed",
        );
    assert!(
        !item["login"].is_null(),
        "the re-delivered row must carry the newly-linked login: {item}"
    );
}
