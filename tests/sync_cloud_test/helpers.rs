//! Shared setup for the `sync_cloud_test` tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use mongodb::bson::oid::ObjectId;

use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;

use simplebash_pos_backend::domain::users::Role;

use simplebash_pos_backend::modules::sync::cloud_capture::run_consumer;

use crate::common;
use tower::ServiceExt;
use uuid::Uuid;

// Cloud sync v2 (PKG-3) against a real MongoDB Atlas cluster: the real router in
// `TENANT_MODE=multi` plus the real change-stream consumer, each test in its own
// throwaway database (`jtroute_<uuid24>`, dropped at the end).
//
// Data is created through the real REST API (so payloads are genuine wire DTOs)
// and then pushed as if by a device of another tenant. Every test uses distinct
// tenants and devices; the tests run one at a time because each opens a change
// stream and the shared Atlas tier is slow under concurrency.

pub(crate) static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json");
    if let Some(t) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {t}"));
    }
    let request = builder
        .body(match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        })
        .expect("the request is well formed");
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the awaited step succeeds");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

pub(crate) fn text(v: &Value, pointer: &str) -> String {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing {pointer} in {v}"))
        .to_string()
}

pub(crate) fn admin_token(app: &common::TestApp, tid: &str) -> String {
    common::mint_token_for_tenant(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
        Some(tid),
    )
}

pub(crate) async fn device_token(app: &common::TestApp, tid: &str, device: &str) -> String {
    common::registered_device_token(app, tid, device).await
}

/// Starts the change-stream consumer and waits until it is really capturing.
/// A change stream only sees writes made after it opened, so a canary write in
/// a throwaway tenant is repeated until one shows up in that tenant's feed.
pub(crate) async fn start(app: &common::TestApp) {
    app.mongo()
        .create_collection("zz_bootstrap")
        .await
        .expect("create bootstrap collection");
    let db = app.db_handle.clone();
    tokio::spawn(async move {
        run_consumer(db).await;
    });

    let admin = admin_token(app, "tnt_canary");
    let dev = device_token(app, "tnt_canary", "dev_canary").await;
    for attempt in 0..60 {
        let (status, body) = call(
            &app.router,
            "POST",
            "/api/inventory/categories",
            Some(&admin),
            Some(json!({ "name": format!("Canary-{attempt}"), "icon": "Box", "color": "blue", "subcategories": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "canary: {body}");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let (status, body) = call(
            &app.router,
            "GET",
            "/api/sync/pull?since=0",
            Some(&dev),
            None,
        )
        .await;
        if status == StatusCode::OK
            && body["data"]["changes"]
                .as_array()
                .is_some_and(|c| !c.is_empty())
        {
            return;
        }
    }
    panic!("the change-stream consumer never captured a canary write");
}

/// Polls `GET /sync/pull` until `pred` accepts the returned changes.
pub(crate) async fn wait_for(
    app: &common::TestApp,
    token: &str,
    since: i64,
    pred: impl Fn(&Vec<Value>) -> bool,
) -> Value {
    for _ in 0..90 {
        let (status, body) = call(
            &app.router,
            "GET",
            &format!("/api/sync/pull?since={since}&limit=500"),
            Some(token),
            None,
        )
        .await;
        if status == StatusCode::OK {
            let changes = body["data"]["changes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if pred(&changes) {
                return body["data"].clone();
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    panic!("timed out waiting for the change feed");
}

pub(crate) async fn push(
    app: &common::TestApp,
    tid: &str,
    device: &str,
    batch: &str,
    changes: Vec<Value>,
) -> (StatusCode, Value) {
    let token = device_token(app, tid, device).await;
    let outbox: Vec<i64> = (1..=changes.len() as i64).collect();
    call(
        &app.router,
        "POST",
        "/api/sync/push",
        Some(&token),
        Some(json!({
            "deviceId": device, "batchId": batch, "baseSeq": 0,
            "changes": changes, "outboxSeqs": outbox,
        })),
    )
    .await
}

/// A copy of a DTO with a fresh legacy id. `_id` is unique across tenants, so a
/// payload seeded in one tenant cannot be pushed unchanged into another.
pub(crate) fn reid(payload: &Value) -> Value {
    let mut copy = payload.clone();
    if copy.get("id").is_some() {
        copy["id"] = json!(ObjectId::new().to_hex());
    }
    copy
}

pub(crate) fn record(resource: &str, payload: &Value, device: &str) -> Value {
    json!({
        "resource": resource, "key": payload["key"], "op": "upsert",
        "version": payload["version"], "updatedAt": payload["updatedAt"],
        "deviceId": device, "payload": payload,
    })
}

pub(crate) fn acks(body: &Value) -> Vec<Value> {
    body["data"]["acks"].as_array().cloned().unwrap_or_default()
}

pub(crate) fn statuses(body: &Value) -> Vec<String> {
    acks(body)
        .iter()
        .map(|a| a["status"].as_str().unwrap_or("").to_string())
        .collect()
}

/// A quotation made through the REST API (a web user, or a device's own UI).
pub(crate) async fn web_quotation(app: &common::TestApp, tid: &str, who: &str) -> Value {
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/quotations",
        Some(&admin_token(app, tid)),
        Some(json!({
            "customer": { "customerName": who },
            "items": [{ "name": "Screen replacement", "unitPriceCents": 10000, "quantity": 2, "discountCents": 0 }],
            "validUntil": "2099-12-31", "terms": "50% advance"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "quotation: {body}");
    body["data"].clone()
}

pub(crate) async fn web_cancel_quotation(app: &common::TestApp, tid: &str, key: &str) {
    let (status, body) = call(
        &app.router,
        "POST",
        &format!("/api/quotations/{key}/cancel"),
        Some(&admin_token(app, tid)),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cancel: {body}");
}

pub(crate) struct Seed {
    pub(crate) category: Value,
    pub(crate) product: Value,
    pub(crate) customer: Value,
    pub(crate) supplier: Value,
    pub(crate) employee: Value,
    pub(crate) invoice: Value,
}

pub(crate) async fn seed_tenant(app: &common::TestApp, tid: &str) -> Seed {
    let token = admin_token(app, tid);
    let uniq = Uuid::new_v4().simple().to_string();
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(&token),
        Some(json!({ "name": format!("Cat-{uniq}"), "icon": "Box", "color": "blue", "subcategories": ["Alpha"] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "category: {body}");
    let category = body["data"].clone();
    let (category_key, subcategory_key) = (
        text(&category, "/key"),
        text(&category, "/subcategories/0/key"),
    );

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/inventory/products",
        Some(&token),
        Some(json!({
            "name": format!("Prod-{uniq}"), "categoryKey": category_key,
            "subcategoryKey": subcategory_key, "minStockThreshold": 3,
            "openingStock": { "quantity": 10, "unitCostCents": 1000, "sellingPriceCents": 2000 },
            "autoGenerateBarcode": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "product: {body}");
    let product = body["data"].clone();

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/customers",
        Some(&token),
        Some(json!({ "name": format!("Cust-{uniq}"), "primaryPhone": format!("077{:07}", Uuid::new_v4().as_u128() % 10_000_000) })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "customer: {body}");
    let customer = body["data"].clone();

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/suppliers",
        Some(&token),
        Some(json!({
            "name": format!("Supp-{uniq}"), "contactPerson": "Test Contact",
            "primaryPhone": "077 123 4567", "address": "123 Test Street",
            "suppliedCategories": ["Phone Parts"], "email": "supplier@example.com",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "supplier: {body}");
    let supplier = body["data"].clone();

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/employees",
        Some(&token),
        Some(json!({
            "name": format!("Emp-{uniq}"), "phone": "0771234567", "role": "technician",
            "defaultSplitType": "percentage", "defaultSplitValue": 20.0,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "employee: {body}");
    let employee = body["data"].clone();

    let (status, body) = call(
        &app.router,
        "POST",
        "/api/billing/sales",
        Some(&token),
        Some(json!({
            "staff": { "cashierName": format!("Cashier-{uniq}") },
            "items": [{ "productKey": product["key"], "sellingPriceCents": 2000, "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 2000 },
            "shopProfileSnapshot": { "tradingName": format!("Shop-{uniq}") },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "sale: {body}");
    let invoice = body["data"]["invoice"].clone();

    Seed {
        category,
        product,
        customer,
        supplier,
        employee,
        invoice,
    }
}

pub(crate) async fn drop_db(app: &common::TestApp) {
    app.mongo().drop().await.ok();
}
