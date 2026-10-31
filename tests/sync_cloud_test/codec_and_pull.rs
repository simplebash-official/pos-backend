//! Codec and pull tests.

use axum::http::StatusCode;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document, doc};
use serde_json::{Value, json};

use simplebash_pos_backend::core::tenancy::{Tenant, with_tenant};

use simplebash_pos_backend::modules::sync::apply_mongo::test_support::{
    encode_for_test, hydrate_for_test,
};

use simplebash_pos_backend::modules::tenants::service::create_tenant;

use crate::common;
use crate::helpers::{
    SERIAL, acks, admin_token, call, device_token, drop_db, push, record, reid, seed_tenant, start,
    statuses, text, wait_for, web_cancel_quotation, web_quotation,
};

/// A stock movement wire payload for `product_id` (hex).
fn movement(key: &str, product_id: &str, delta: i64) -> Value {
    let now = chrono::Utc::now().to_rfc3339();
    json!({
        "id": ObjectId::new().to_hex(), "key": key, "productId": product_id,
        "quantityDelta": delta, "type": "manual_adjustment",
        "createdAt": now, "updatedAt": now, "version": 1,
    })
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

// ---------------------------------------------------------------------------
// The codec's field tables must match the real models
// ---------------------------------------------------------------------------

#[tokio::test]
async fn codec_round_trips_real_documents() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    seed_tenant(&app, "tnt_codec").await;
    // One active and one cancelled quotation: the cancelled one carries a bumped version.
    web_quotation(&app, "tnt_codec", "Active customer").await;
    let cancelled = web_quotation(&app, "tnt_codec", "Cancelled customer").await;
    web_cancel_quotation(&app, "tnt_codec", &text(&cancelled, "/key")).await;
    let tenant = Tenant::id("tnt_codec").expect("the tenant id is valid");

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
        ("quotations", "quotations"),
    ] {
        let mut cursor = mongodb::Collection::<Document>::find(
            &app.mongo().collection::<Document>(table),
            doc! { "tenant_id": "tnt_codec" },
        )
        .await
        .expect("the awaited step succeeds");
        use futures_util::TryStreamExt;
        while let Some(raw) = cursor.try_next().await.expect("the awaited step succeeds") {
            let payload = with_tenant(
                tenant.clone(),
                hydrate_for_test(&app.db_handle, resource, raw.clone()),
            )
            .await
            .expect("the awaited step succeeds");
            let encoded = encode_for_test(resource, &payload).expect("the step succeeds");
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
    assert!(checked >= 10, "round-tripped only {checked} documents");
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
    let other = device_token(&app, "tnt_b", "dev_b2").await;
    let feed = wait_for(&app, &other, 0, |c| {
        c.iter()
            .any(|x| x["resource"] == "products" && x["key"] == key)
    })
    .await;
    let changes = feed["changes"].as_array().expect("the value is an array");
    assert!(
        changes
            .iter()
            .any(|c| c["resource"] == "customers" && c["originDeviceId"] == "dev_b1"),
        "pushed rows must carry their origin: {feed}"
    );
    let own = device_token(&app, "tnt_b", "dev_b1").await;
    let own_feed = wait_for(&app, &own, 0, |_| true).await;
    assert!(
        own_feed["changes"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .all(|c| c["originDeviceId"] != "dev_b1"),
        "no echo to the origin device"
    );

    // Tenant A's devices never see B's writes, and B's rows are stamped B.
    let a_dev = device_token(&app, "tnt_a", "dev_a1").await;
    let a_feed = wait_for(&app, &a_dev, 0, |c| !c.is_empty()).await;
    assert!(
        a_feed["changes"]
            .as_array()
            .expect("the value is an array")
            .iter()
            .all(|c| c["originDeviceId"] != "dev_b1" && c["deviceId"] != "dev_b1"),
        "tenant A must not receive tenant B's changes"
    );
    let b_products = app
        .mongo()
        .collection::<Document>("products")
        .count_documents(doc! { "tenant_id": "tnt_b" })
        .await
        .expect("the awaited step succeeds");
    assert_eq!(b_products, 1);
    drop_db(&app).await;
}

#[tokio::test]
async fn a_lost_stream_position_is_recovered_and_missed_writes_are_replayed() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let tid = "tnt_lostpos";
    let admin = admin_token(&app, tid);

    // A write made while the consumer was "away": nothing is watching yet, so only
    // the catch-up can deliver it.
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(&admin),
        Some(json!({ "name": "While Away", "icon": "Box", "color": "blue", "subcategories": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // The consumer's saved position is one the server can never resume from.
    app.mongo()
        .collection::<mongodb::bson::Document>("sync_consumer")
        .insert_one(mongodb::bson::doc! {
            "_id": "consumer",
            "holder": "dead-instance",
            "lease_until": mongodb::bson::DateTime::from_millis(0),
            "resume_token": { "_data": "8200000000000000000000000000" },
            "resume_token_at": mongodb::bson::DateTime::from_millis(
                chrono::Utc::now().timestamp_millis() - 60_000
            ),
        })
        .await
        .expect("plant the lost position");

    start(&app).await;

    // It recovered (no endless retry), and the category written while away was replayed.
    let dev = device_token(&app, tid, "dev_lostpos").await;
    let feed = wait_for(&app, &dev, 0, |changes| {
        changes
            .iter()
            .any(|c| c.pointer("/payload/name").and_then(Value::as_str) == Some("While Away"))
    })
    .await;
    assert!(feed["changes"].as_array().is_some_and(|c| !c.is_empty()));

    // The bad position was dropped and a good one is saved again later; it is no longer the planted one.
    let lease = app
        .mongo()
        .collection::<mongodb::bson::Document>("sync_consumer")
        .find_one(mongodb::bson::doc! { "_id": "consumer" })
        .await
        .expect("the awaited step succeeds")
        .expect("lease document");
    assert_ne!(
        lease
            .get_document("resume_token")
            .ok()
            .and_then(|t| t.get_str("_data").ok()),
        Some("8200000000000000000000000000")
    );
    drop_db(&app).await;
}

#[tokio::test]
async fn web_write_appears_in_pull_without_an_origin_device() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let seed = seed_tenant(&app, "tnt_w").await;
    let key = text(&seed.product, "/key");

    let dev = device_token(&app, "tnt_w", "dev_w1").await;
    let feed = wait_for(&app, &dev, 0, |c| {
        c.iter()
            .any(|x| x["resource"] == "products" && x["key"] == key)
    })
    .await;
    let change = feed["changes"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| c["resource"] == "products" && c["key"] == key)
        .expect("a match exists");
    // No device made it: every device receives it.
    assert_eq!(
        change["originDeviceId"], "cloud",
        "web write origin: {change}"
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
    assert!(feed["nextSeq"].as_i64().expect("the value is an integer") > 0);
    drop_db(&app).await;
}

#[tokio::test]
async fn revoked_and_non_device_callers_are_refused() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;

    // A device push registers the device.
    let (status, body) = push(&app, "tnt_r", "dev_r1", "batch-1", vec![]).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    app.mongo()
        .collection::<Document>("sync_device_state")
        .update_one(
            doc! { "tenant_id": "tnt_r", "device_id": "dev_r1" },
            doc! { "$set": { "revoked": true } },
        )
        .await
        .expect("the awaited step succeeds");

    let (status, body) = push(&app, "tnt_r", "dev_r1", "batch-2", vec![]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "DEVICE_REVOKED");
    let token = device_token(&app, "tnt_r", "dev_r1").await;
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
        Some(&device_token(&app, "tnt_r", "dev_r2").await),
        Some(
            json!({ "deviceId": "dev_someone_else", "batchId": "b", "baseSeq": 0, "changes": [] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "DEVICE_MISMATCH");
    drop_db(&app).await;
}

/// A desktop app older than the cloud's minimum (or one that never told the
/// cloud its version, i.e. every app from before the check) is held back with a
/// retryable 503: older apps keep a change they get a 5xx for and drop one they
/// get a 4xx for, so its waiting sales survive until it updates.
#[tokio::test]
async fn an_outdated_desktop_app_cannot_push_or_pull_until_it_updates() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let tid = "tnt_old";
    let token = common::mint_device_token(&app.config, tid, "dev_old1");
    let product = json!({
        "key": "prod_from_old_app", "name": "Old app product", "version": 1,
        "updatedAt": "2026-10-01T00:00:00Z",
    });
    let push_body = json!({
        "deviceId": "dev_old1", "batchId": "b-old", "baseSeq": 0,
        "changes": [record("products", &product, "dev_old1")], "outboxSeqs": [1],
    });
    let refused = |(status, body): (StatusCode, Value)| {
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(body["code"], "APP_UPDATE_REQUIRED", "{body}");
    };
    let register = |version: &'static str| {
        let token = token.clone();
        let router = app.router.clone();
        async move {
            call(
                &router,
                "POST",
                "/api/sync/devices/register",
                Some(&token),
                Some(
                    json!({ "deviceId": "dev_old1", "deviceName": "Till", "appVersion": version }),
                ),
            )
            .await
        }
    };
    let try_all = || {
        let token = token.clone();
        let router = app.router.clone();
        let push_body = push_body.clone();
        async move {
            [
                call(
                    &router,
                    "POST",
                    "/api/sync/push",
                    Some(&token),
                    Some(push_body),
                )
                .await,
                call(&router, "GET", "/api/sync/pull?since=0", Some(&token), None).await,
                call(
                    &router,
                    "GET",
                    "/api/sync/snapshot?limit=10",
                    Some(&token),
                    None,
                )
                .await,
            ]
        }
    };

    // An app that never registered a version (every app before this check).
    try_all().await.into_iter().for_each(refused);
    // One that registers an older version, or something that is not a version.
    refused(register("1.12.0").await);
    refused(register("1.13.0").await);
    refused(register("1.22.0").await);
    refused(register("1.23.0").await);
    refused(register("not-a-version").await);
    try_all().await.into_iter().for_each(refused);
    let stored = app
        .mongo()
        .collection::<mongodb::bson::Document>("products")
        .count_documents(doc! { "key": "prod_from_old_app" })
        .await
        .expect("the awaited step succeeds");
    assert_eq!(stored, 0, "nothing from the outdated app was applied");

    // After the update it registers its new version and syncs as usual.
    let (status, body) = register("1.24.0").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    for (status, body) in try_all().await {
        assert_eq!(status, StatusCode::OK, "{body}");
    }
    drop_db(&app).await;
}

// ---------------------------------------------------------------------------
// The shop's own first-time setup, as seen by devices
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_device_sees_whether_the_shop_is_set_up_and_can_mark_it_once() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let shop = create_tenant(&app.db_handle, "shop-setup-flag", "Setup Flag Shop")
        .await
        .expect("the awaited step succeeds");
    let tid = shop.key.clone();
    // Registers the device (a push by a device is how it joins the shop).
    let (status, body) = push(&app, &tid, "dev_setup", "b-1", vec![]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let token = device_token(&app, &tid, "dev_setup").await;

    // A freshly provisioned shop has not chosen demo vs clean data yet.
    let (status, body) = call(&app.router, "GET", "/api/sync/status", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["setupCompleted"], false);
    assert_eq!(body["data"]["sampleDataLoaded"], false);

    // The device finished the setup: the cloud records it.
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/sync/setup-complete",
        Some(&token),
        Some(json!({ "sampleDataLoaded": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["setupCompleted"], true);
    assert_eq!(body["data"]["changed"], true);

    let (_, body) = call(&app.router, "GET", "/api/sync/status", Some(&token), None).await;
    assert_eq!(body["data"]["setupCompleted"], true);
    assert_eq!(body["data"]["sampleDataLoaded"], true);

    // Repeating it, even with different details, changes nothing.
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/sync/setup-complete",
        Some(&token),
        Some(json!({ "sampleDataLoaded": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["changed"], false);
    let (_, body) = call(&app.router, "GET", "/api/sync/status", Some(&token), None).await;
    assert_eq!(body["data"]["sampleDataLoaded"], true, "never downgraded");

    // A web owner session is not a device and cannot use it.
    let owner = admin_token(&app, &tid);
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/sync/setup-complete",
        Some(&owner),
        Some(json!({ "sampleDataLoaded": false })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "DEVICE_TOKEN_REQUIRED");
    drop_db(&app).await;
}
