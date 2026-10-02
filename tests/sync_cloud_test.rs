// Cloud sync v2 (PKG-3) against a real MongoDB Atlas cluster: the real router in
// `TENANT_MODE=multi` plus the real change-stream consumer, each test in its own
// throwaway database (`jtroute_<uuid24>`, dropped at the end).
//
// Data is created through the real REST API (so payloads are genuine wire DTOs)
// and then pushed as if by a device of another tenant. Every test uses distinct
// tenants and devices; the tests run one at a time because each opens a change
// stream and the shared Atlas tier is slow under concurrency.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use mongodb::bson::{Bson, Document, doc, oid::ObjectId};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::{
        constants::roles,
        tenancy::{Tenant, with_tenant},
    },
    domain::users::Role,
    modules::sync::{
        apply_mongo::test_support::{encode_for_test, hydrate_for_test},
        cloud_capture::run_consumer,
        compaction::compact_tenant,
    },
    modules::tenants::service::create_tenant,
};
use tower::ServiceExt;
use uuid::Uuid;

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn call(
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
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn text(v: &Value, pointer: &str) -> String {
    v.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing {pointer} in {v}"))
        .to_string()
}

fn admin_token(app: &common::TestApp, tid: &str) -> String {
    common::mint_token_for_tenant(
        &app.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
        Some(tid),
    )
}

fn device_token(app: &common::TestApp, tid: &str, device: &str) -> String {
    common::mint_device_token(&app.config, tid, device)
}

/// Starts the change-stream consumer and waits until it is really capturing.
/// A change stream only sees writes made after it opened, so a canary write in
/// a throwaway tenant is repeated until one shows up in that tenant's feed.
async fn start(app: &common::TestApp) {
    app.db
        .create_collection("zz_bootstrap")
        .await
        .expect("create bootstrap collection");
    let db = app.db_handle.clone();
    tokio::spawn(async move {
        run_consumer(db).await;
    });

    let admin = admin_token(app, "tnt_canary");
    let dev = device_token(app, "tnt_canary", "dev_canary");
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
async fn wait_for(
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

async fn push(
    app: &common::TestApp,
    tid: &str,
    device: &str,
    batch: &str,
    changes: Vec<Value>,
) -> (StatusCode, Value) {
    let token = device_token(app, tid, device);
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
fn reid(payload: &Value) -> Value {
    let mut copy = payload.clone();
    if copy.get("id").is_some() {
        copy["id"] = json!(ObjectId::new().to_hex());
    }
    copy
}

fn record(resource: &str, payload: &Value, device: &str) -> Value {
    json!({
        "resource": resource, "key": payload["key"], "op": "upsert",
        "version": payload["version"], "updatedAt": payload["updatedAt"],
        "deviceId": device, "payload": payload,
    })
}

fn acks(body: &Value) -> Vec<Value> {
    body["data"]["acks"].as_array().cloned().unwrap_or_default()
}

fn statuses(body: &Value) -> Vec<String> {
    acks(body)
        .iter()
        .map(|a| a["status"].as_str().unwrap_or("").to_string())
        .collect()
}

/// A stock movement wire payload for `product_id` (hex).
fn movement(key: &str, product_id: &str, delta: i64) -> Value {
    let now = chrono::Utc::now().to_rfc3339();
    json!({
        "id": ObjectId::new().to_hex(), "key": key, "productId": product_id,
        "quantityDelta": delta, "type": "manual_adjustment",
        "createdAt": now, "updatedAt": now, "version": 1,
    })
}

struct Seed {
    category: Value,
    product: Value,
    customer: Value,
    supplier: Value,
    employee: Value,
    invoice: Value,
}

async fn seed_tenant(app: &common::TestApp, tid: &str) -> Seed {
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
            "subcategoryKey": subcategory_key, "costPriceCents": 1000,
            "sellingPriceCents": 2000, "stockQuantity": 10, "minStockThreshold": 3,
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
            "items": [{ "productKey": product["key"], "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
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

fn bson_number(b: &Bson) -> Option<f64> {
    match b {
        Bson::Int32(v) => Some(f64::from(*v)),
        Bson::Int64(v) => Some(*v as f64),
        Bson::Double(v) => Some(*v),
        _ => None,
    }
}

fn bson_equal(a: &Bson, b: &Bson) -> bool {
    if let (Some(x), Some(y)) = (bson_number(a), bson_number(b)) {
        return (x - y).abs() < 1e-9;
    }
    match (a, b) {
        (Bson::Document(x), Bson::Document(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| bson_equal(v, w)))
        }
        (Bson::Array(x), Bson::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| bson_equal(p, q))
        }
        _ => a == b,
    }
}

async fn drop_db(app: &common::TestApp) {
    app.db.drop().await.ok();
}

// ---------------------------------------------------------------------------
// The codec's field tables must match the real models
// ---------------------------------------------------------------------------

#[tokio::test]
async fn codec_round_trips_real_documents() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    seed_tenant(&app, "tnt_codec").await;
    let tenant = Tenant::id("tnt_codec").unwrap();

    // Exactly the columns the merge rules manage themselves.
    const MANAGED: &[&str] = &[
        "_id",
        "tenant_id",
        "updated_by_device",
        "version",
        "created_at",
        "updated_at",
        "deleted_at",
    ];
    let mut checked = 0;
    for (resource, table) in [
        ("categories", "categories"),
        ("suppliers", "suppliers"),
        ("products", "products"),
        ("employees", "employees"),
        ("customers", "customers"),
        ("stockMovements", "stock_movements"),
        ("invoices", "invoices"),
        ("payments", "payments"),
    ] {
        let mut cursor = mongodb::Collection::<Document>::find(
            &app.db.collection::<Document>(table),
            doc! { "tenant_id": "tnt_codec" },
        )
        .await
        .unwrap();
        use futures_util::TryStreamExt;
        while let Some(raw) = cursor.try_next().await.unwrap() {
            let payload = with_tenant(
                tenant.clone(),
                hydrate_for_test(&app.db_handle, resource, raw.clone()),
            )
            .await
            .unwrap();
            let encoded = encode_for_test(resource, &payload).unwrap();
            checked += 1;

            for (field, value) in &encoded {
                if field == "_id" {
                    // Categories carry no legacy id on the wire; a fresh one is minted.
                    if payload.get("id").is_some() {
                        assert_eq!(raw.get("_id"), Some(value), "{resource}._id");
                    }
                    continue;
                }
                let stored = raw
                    .get(field)
                    .unwrap_or_else(|| panic!("{resource}: encoded field '{field}' is not stored"));
                assert!(
                    bson_equal(stored, value),
                    "{resource}.{field}: stored {stored:?} != encoded {value:?}"
                );
            }
            for (field, _) in &raw {
                if MANAGED.contains(&field.as_str()) {
                    continue;
                }
                assert!(
                    encoded.contains_key(field),
                    "{resource}: stored field '{field}' would be lost by a push (payload: {payload})"
                );
            }
        }
    }
    assert!(checked >= 8, "round-tripped only {checked} documents");
    drop_db(&app).await;
}

// ---------------------------------------------------------------------------
// Push, retry, ordering, echo filtering and tenant isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn push_pull_roundtrip_dedup_and_isolation() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let seed = seed_tenant(&app, "tnt_a").await;

    // Tenant B's device pushes A's DTOs. The product carries a forged stock
    // figure and its movement comes FIRST in the batch: derived stock must come
    // from the ledger, and parents must be applied before children.
    let mut product = reid(&seed.product);
    product["stockQuantity"] = json!(999);
    let product_id = text(&product, "/id");
    let changes = vec![
        record(
            "stockMovements",
            &movement("sm_test_open", &product_id, 10),
            "dev_b1",
        ),
        record("products", &product, "dev_b1"),
        record("categories", &seed.category, "dev_b1"),
        record("customers", &reid(&seed.customer), "dev_b1"),
        record("suppliers", &reid(&seed.supplier), "dev_b1"),
        record("employees", &reid(&seed.employee), "dev_b1"),
    ];
    let (status, first) = push(&app, "tnt_b", "dev_b1", "batch-1", changes.clone()).await;
    assert_eq!(status, StatusCode::OK, "push: {first}");
    assert!(
        statuses(&first).iter().all(|s| s == "applied"),
        "first push: {first}"
    );

    let b_admin = admin_token(&app, "tnt_b");
    let (status, body) = call(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        Some(&b_admin),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["stockQuantity"], 10,
        "stock must equal the ledger, not the payload"
    );
    assert_eq!(body["data"]["name"], product["name"]);
    assert_eq!(body["data"]["sku"], product["sku"]);

    // Retrying the SAME batch replays the recorded acks and changes nothing.
    let (status, replay) = push(&app, "tnt_b", "dev_b1", "batch-1", changes.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(acks(&replay), acks(&first));
    // A NEW batch id with identical content is recognised change by change.
    let (status, again) = push(&app, "tnt_b", "dev_b1", "batch-2", changes).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert!(
        statuses(&again).iter().all(|s| s == "duplicate"),
        "re-push: {again}"
    );
    let (_, body) = call(
        &app.router,
        "GET",
        &format!("/api/inventory/products/{product_id}"),
        Some(&b_admin),
        None,
    )
    .await;
    assert_eq!(body["data"]["stockQuantity"], 10);

    // Another device of B sees the pushed rows, attributed to dev_b1; dev_b1
    // itself never gets its own writes back but its cursor still advances.
    let key = text(&product, "/key");
    let other = device_token(&app, "tnt_b", "dev_b2");
    let feed = wait_for(&app, &other, 0, |c| {
        c.iter()
            .any(|x| x["resource"] == "products" && x["key"] == key)
    })
    .await;
    let changes = feed["changes"].as_array().unwrap();
    assert!(
        changes
            .iter()
            .any(|c| c["resource"] == "customers" && c["originDeviceId"] == "dev_b1"),
        "pushed rows must carry their origin: {feed}"
    );
    let own = device_token(&app, "tnt_b", "dev_b1");
    let own_feed = wait_for(&app, &own, 0, |_| true).await;
    assert!(
        own_feed["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["originDeviceId"] != "dev_b1"),
        "no echo to the origin device"
    );

    // Tenant A's devices never see B's writes, and B's rows are stamped B.
    let a_dev = device_token(&app, "tnt_a", "dev_a1");
    let a_feed = wait_for(&app, &a_dev, 0, |c| !c.is_empty()).await;
    assert!(
        a_feed["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["originDeviceId"] != "dev_b1" && c["deviceId"] != "dev_b1"),
        "tenant A must not receive tenant B's changes"
    );
    let b_products = app
        .db
        .collection::<Document>("products")
        .count_documents(doc! { "tenant_id": "tnt_b" })
        .await
        .unwrap();
    assert_eq!(b_products, 1);
    drop_db(&app).await;
}

#[tokio::test]
async fn web_write_appears_in_pull_without_an_origin_device() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let seed = seed_tenant(&app, "tnt_w").await;
    let key = text(&seed.product, "/key");

    let dev = device_token(&app, "tnt_w", "dev_w1");
    let feed = wait_for(&app, &dev, 0, |c| {
        c.iter()
            .any(|x| x["resource"] == "products" && x["key"] == key)
    })
    .await;
    let change = feed["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["resource"] == "products" && c["key"] == key)
        .unwrap();
    assert!(
        change["originDeviceId"].is_null(),
        "web write has no device: {change}"
    );
    assert_eq!(change["op"], "upsert");
    assert_eq!(change["payload"]["name"], seed.product["name"]);
    // Invoices from the sale travel too, in wire (camelCase) form.
    let invoice_key = text(&seed.invoice, "/key");
    let feed = wait_for(&app, &dev, 0, |c| {
        c.iter()
            .any(|x| x["resource"] == "invoices" && x["key"] == invoice_key)
    })
    .await;
    assert!(feed["nextSeq"].as_i64().unwrap() > 0);
    drop_db(&app).await;
}

#[tokio::test]
async fn revoked_and_non_device_callers_are_refused() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;

    // A device push registers the device.
    let (status, body) = push(&app, "tnt_r", "dev_r1", "batch-1", vec![]).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    app.db
        .collection::<Document>("sync_device_state")
        .update_one(
            doc! { "tenant_id": "tnt_r", "device_id": "dev_r1" },
            doc! { "$set": { "revoked": true } },
        )
        .await
        .unwrap();

    let (status, body) = push(&app, "tnt_r", "dev_r1", "batch-2", vec![]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "DEVICE_REVOKED");
    let token = device_token(&app, "tnt_r", "dev_r1");
    let (status, body) = call(
        &app.router,
        "GET",
        "/api/sync/pull?since=0",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "DEVICE_REVOKED");

    // A web owner session is not a device.
    let owner = admin_token(&app, "tnt_r");
    let (status, body) = call(
        &app.router,
        "GET",
        "/api/sync/pull?since=0",
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "DEVICE_TOKEN_REQUIRED");

    // The push must name the device its token carries.
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/sync/push",
        Some(&device_token(&app, "tnt_r", "dev_r2")),
        Some(
            json!({ "deviceId": "dev_someone_else", "batchId": "b", "baseSeq": 0, "changes": [] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "DEVICE_MISMATCH");
    drop_db(&app).await;
}

// ---------------------------------------------------------------------------
// Merge behaviour through the push endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lww_loser_is_recorded_and_newer_wins() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let seed = seed_tenant(&app, "tnt_src").await;

    let base = chrono::DateTime::parse_from_rfc3339(&text(&seed.customer, "/updatedAt")).unwrap();
    let at = |secs: i64| (base + chrono::Duration::seconds(secs)).to_rfc3339();
    let customer = reid(&seed.customer);
    let variant = |name: &str, secs: i64, device: &str| {
        let mut payload = customer.clone();
        payload["name"] = json!(name);
        payload["updatedAt"] = json!(at(secs));
        record("customers", &payload, device)
    };

    let (_, body) = push(
        &app,
        "tnt_l",
        "dev_l1",
        "b1",
        vec![variant("V2", 10, "dev_l1")],
    )
    .await;
    assert_eq!(statuses(&body), ["applied"], "{body}");

    // An older edit from another device loses and is kept for review.
    let (_, body) = push(
        &app,
        "tnt_l",
        "dev_l2",
        "b2",
        vec![variant("OLD", 0, "dev_l2")],
    )
    .await;
    assert_eq!(statuses(&body), ["conflict"], "{body}");
    assert_eq!(acks(&body)[0]["reason"], "LWW_LOSER");
    let key = text(&seed.customer, "/key");
    let stored = app
        .db
        .collection::<Document>("customers")
        .find_one(doc! { "tenant_id": "tnt_l", "key": &key })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.get_str("name").unwrap(), "V2");
    let conflicts = app
        .db
        .collection::<Document>("sync_conflicts")
        .count_documents(doc! { "tenant_id": "tnt_l", "kind": "LWW_LOSER" })
        .await
        .unwrap();
    assert_eq!(conflicts, 1);

    // A newer edit wins.
    let (_, body) = push(
        &app,
        "tnt_l",
        "dev_l2",
        "b3",
        vec![variant("V3", 20, "dev_l2")],
    )
    .await;
    assert_eq!(statuses(&body), ["applied"], "{body}");
    let stored = app
        .db
        .collection::<Document>("customers")
        .find_one(doc! { "tenant_id": "tnt_l", "key": &key })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.get_str("name").unwrap(), "V3");
    assert_eq!(stored.get_str("updated_by_device").unwrap(), "dev_l2");
    drop_db(&app).await;
}

#[tokio::test]
async fn unique_collision_is_renamed_and_reported() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let seed = seed_tenant(&app, "tnt_src").await;

    let clone_product = |key: &str| {
        let mut p = reid(&seed.product);
        p["key"] = json!(key);
        p["sku"] = json!("DUP-0001");
        p["barcode"] = Value::Null;
        record("products", &p, "dev_u1")
    };
    let (_, body) = push(
        &app,
        "tnt_u",
        "dev_u1",
        "b1",
        vec![clone_product("prd_dup_1"), clone_product("prd_dup_2")],
    )
    .await;
    assert!(statuses(&body).iter().all(|s| s == "applied"), "{body}");

    let mut skus = Vec::new();
    let mut cursor = mongodb::Collection::<Document>::find(
        &app.db.collection::<Document>("products"),
        doc! { "tenant_id": "tnt_u" },
    )
    .await
    .unwrap();
    use futures_util::TryStreamExt;
    while let Some(d) = cursor.try_next().await.unwrap() {
        skus.push(d.get_str("sku").unwrap().to_string());
    }
    skus.sort();
    assert_eq!(skus.len(), 2);
    assert_eq!(skus[0], "DUP-0001");
    assert!(skus[1].starts_with("DUP-0001-"), "{skus:?}");
    let conflicts = app
        .db
        .collection::<Document>("sync_conflicts")
        .count_documents(doc! { "tenant_id": "tnt_u", "kind": "UNIQUE_VIOLATION" })
        .await
        .unwrap();
    assert_eq!(conflicts, 1);
    drop_db(&app).await;
}

// ---------------------------------------------------------------------------
// Compaction, snapshot bootstrap and pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compaction_expires_the_cursor_and_a_snapshot_recovers() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let seed = seed_tenant(&app, "tnt_c").await;
    let dev = device_token(&app, "tnt_c", "dev_c1");
    let feed = wait_for(&app, &dev, 0, |c| c.len() >= 5).await;
    let newest = feed["nextSeq"].as_i64().unwrap();
    assert!(newest >= 5);

    let dropped = with_tenant(
        Tenant::id("tnt_c").unwrap(),
        compact_tenant(&app.db_handle, 0),
    )
    .await
    .unwrap();
    assert!(dropped >= 5, "dropped {dropped}");

    let (status, body) = call(
        &app.router,
        "GET",
        "/api/sync/pull?since=0",
        Some(&dev),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::GONE, "{body}");
    assert_eq!(body["code"], "CURSOR_EXPIRED");

    // Bootstrap: the snapshot has the live rows and the seq to resume from.
    let (status, snap) = call(
        &app.router,
        "GET",
        "/api/sync/snapshot?limit=1000",
        Some(&dev),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snap}");
    let keys: Vec<String> = snap["data"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap().to_string())
        .collect();
    assert!(keys.contains(&text(&seed.product, "/key")));
    assert!(keys.contains(&text(&seed.customer, "/key")));
    let as_of = snap["data"]["asOfSeq"].as_i64().unwrap();
    assert!(as_of >= newest);
    let (status, body) = call(
        &app.router,
        "GET",
        &format!("/api/sync/pull?since={as_of}"),
        Some(&dev),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    drop_db(&app).await;
}

#[tokio::test]
async fn snapshot_pagination_is_consistent() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    seed_tenant(&app, "tnt_p").await;
    let dev = device_token(&app, "tnt_p", "dev_p1");

    let mut expected = 0u64;
    for table in [
        "categories",
        "suppliers",
        "products",
        "supplier_products",
        "employees",
        "customers",
        "purchases",
        "stock_movements",
        "product_serials",
        "repairs",
        "print_jobs",
        "invoices",
        "payments",
        "credit_notes",
    ] {
        expected += app
            .db
            .collection::<Document>(table)
            .count_documents(doc! { "tenant_id": "tnt_p", "deleted_at": Bson::Null })
            .await
            .unwrap();
    }
    assert!(expected >= 7, "seed produced {expected} live rows");

    let mut seen = std::collections::BTreeSet::new();
    let mut page: Option<String> = None;
    let mut as_of = None;
    let mut pages = 0;
    loop {
        let uri = match &page {
            Some(p) => format!("/api/sync/snapshot?limit=2&page={p}"),
            None => "/api/sync/snapshot?limit=2".to_string(),
        };
        let (status, body) = call(&app.router, "GET", &uri, Some(&dev), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let data = &body["data"];
        let seq = data["asOfSeq"].as_i64().unwrap();
        assert_eq!(
            *as_of.get_or_insert(seq),
            seq,
            "asOfSeq must not move between pages"
        );
        for c in data["changes"].as_array().unwrap() {
            let id = format!("{}:{}", c["resource"], c["key"]);
            assert!(
                seen.insert(id.clone()),
                "duplicate record across pages: {id}"
            );
        }
        pages += 1;
        match data["nextPage"].as_str() {
            Some(next) => page = Some(next.to_string()),
            None => break,
        }
        assert!(pages < 200, "pagination did not terminate");
    }
    assert!(pages >= 3, "expected several pages, got {pages}");
    assert_eq!(seen.len() as u64, expected);
    drop_db(&app).await;
}

// ---------------------------------------------------------------------------
// Users: a cashier pushed by a device can log in on the cloud web app
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_pushed_cashier_logs_in_by_shop_code_and_a_web_delete_reaches_devices() {
    use argon2::{
        Argon2,
        password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
    };
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;

    let shop = create_tenant(&app.db_handle, "shop-cashier", "Cashier Shop")
        .await
        .unwrap();
    let other = create_tenant(&app.db_handle, "shop-other", "Other Shop")
        .await
        .unwrap();
    let tid = shop.key.clone();

    let hash = Argon2::default()
        .hash_password(b"cashier-pass-9", &SaltString::generate(&mut OsRng))
        .unwrap()
        .to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let id = ObjectId::new().to_hex();
    let payload = json!({
        "id": id, "key": "usr_cashier_cloud", "name": "Cloud Cashier", "email": "cashier@shop.test",
        "passwordHash": hash, "role": "staff", "isActive": true, "employeeKey": null,
        "createdAt": now, "updatedAt": now, "version": 1, "deletedAt": null,
    });

    // A device of the shop pushes the cashier.
    let (status, pushed) = push(
        &app,
        &tid,
        "dev_u1",
        "batch-u1",
        vec![record("users", &payload, "dev_u1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "push: {pushed}");
    assert_eq!(statuses(&pushed), ["applied"], "{pushed}");

    // The cashier logs in on the cloud by shop code and gets a token for THAT shop.
    let login = |shop_code: &str, password: &str| json!({ "email": "cashier@shop.test", "password": password, "shopCode": shop_code });
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/auth/login",
        None,
        Some(login("shop-cashier", "cashier-pass-9")),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "cashier login: {body}");
    assert_eq!(body["data"]["user"]["role"], "staff");
    assert!(
        body["data"]["user"].get("passwordHash").is_none(),
        "no REST response carries the hash"
    );
    let (status, _) = call(
        &app.router,
        "POST",
        "/api/auth/login",
        None,
        Some(login("shop-cashier", "wrong-password")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // The same credentials do not work in another shop.
    assert_eq!(other.shop_code, "shop-other");
    let (status, _) = call(
        &app.router,
        "POST",
        "/api/auth/login",
        None,
        Some(login("shop-other", "cashier-pass-9")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Another device of the shop pulls the cashier, hash included (that is what
    // lets the login work there), through the authenticated sync transport only.
    let observer = device_token(&app, &tid, "dev_u2");
    let feed = wait_for(&app, &observer, 0, |changes| {
        changes
            .iter()
            .any(|c| c["resource"] == "users" && c["key"] == "usr_cashier_cloud")
    })
    .await;
    let pulled = feed["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["resource"] == "users" && c["key"] == "usr_cashier_cloud" && c["op"] == "upsert"
        })
        .expect("the cashier is in the change feed");
    assert_eq!(pulled["payload"]["passwordHash"], hash);
    assert_eq!(pulled["payload"]["id"], id);
    let cursor = feed["nextSeq"].as_i64().unwrap();

    // The owner deletes the cashier on the web: it is a tombstone the change
    // stream carries to devices as a delete, and the login stops working.
    let admin = admin_token(&app, &tid);
    let (status, body) = call(
        &app.router,
        "DELETE",
        &format!("/api/users/{id}"),
        Some(&admin),
        None,
    )
    .await;
    assert!(status.is_success(), "web delete: {status} {body}");
    wait_for(&app, &observer, cursor, |changes| {
        changes.iter().any(|c| {
            c["resource"] == "users" && c["key"] == "usr_cashier_cloud" && c["op"] == "delete"
        })
    })
    .await;
    let (status, _) = call(
        &app.router,
        "POST",
        "/api/auth/login",
        None,
        Some(login("shop-cashier", "cashier-pass-9")),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a deleted cashier cannot log in"
    );

    app.db.drop().await.ok();
}
