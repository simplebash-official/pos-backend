//! Push and conflicts tests.

use axum::http::StatusCode;

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document, doc};
use serde_json::{Value, json};

use simplebash_pos_backend::core::tenancy::{Tenant, with_tenant};
use simplebash_pos_backend::modules::settings::service::ensure_shop_profile;

use simplebash_pos_backend::modules::sync::apply_mongo::test_support::hydrate_for_test;

use simplebash_pos_backend::modules::sync::compaction::compact_tenant;
use simplebash_pos_backend::modules::tenants::service::create_tenant;

use crate::common;
use crate::helpers::{
    SERIAL, acks, admin_token, call, device_token, drop_db, push, record, reid, seed_tenant, start,
    statuses, text, wait_for, web_cancel_quotation, web_quotation,
};

/// The stored cloud document of a quotation.
async fn stored_quotation(app: &common::TestApp, tid: &str, key: &str) -> Document {
    app.mongo()
        .collection::<Document>("quotations")
        .find_one(doc! { "tenant_id": tid, "key": key })
        .await
        .expect("the awaited step succeeds")
        .unwrap_or_else(|| panic!("quotation {key} is not stored for {tid}"))
}

// ---------------------------------------------------------------------------
// Merge behaviour through the push endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lww_loser_is_recorded_and_newer_wins() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    let seed = seed_tenant(&app, "tnt_src").await;

    let base = chrono::DateTime::parse_from_rfc3339(&text(&seed.customer, "/updatedAt"))
        .expect("the step succeeds");
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
        .mongo()
        .collection::<Document>("customers")
        .find_one(doc! { "tenant_id": "tnt_l", "key": &key })
        .await
        .expect("the awaited step succeeds")
        .expect("the step succeeds");
    assert_eq!(
        stored
            .get_str("name")
            .expect("the document has this string field"),
        "V2"
    );
    let conflicts = app
        .mongo()
        .collection::<Document>("sync_conflicts")
        .count_documents(doc! { "tenant_id": "tnt_l", "kind": "LWW_LOSER" })
        .await
        .expect("the awaited step succeeds");
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
        .mongo()
        .collection::<Document>("customers")
        .find_one(doc! { "tenant_id": "tnt_l", "key": &key })
        .await
        .expect("the awaited step succeeds")
        .expect("the step succeeds");
    assert_eq!(
        stored
            .get_str("name")
            .expect("the document has this string field"),
        "V3"
    );
    assert_eq!(
        stored
            .get_str("updated_by_device")
            .expect("the document has this string field"),
        "dev_l2"
    );
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
        &app.mongo().collection::<Document>("products"),
        doc! { "tenant_id": "tnt_u" },
    )
    .await
    .expect("the awaited step succeeds");
    use futures_util::TryStreamExt;
    while let Some(d) = cursor.try_next().await.expect("the awaited step succeeds") {
        skus.push(
            d.get_str("sku")
                .expect("the document has this string field")
                .to_string(),
        );
    }
    skus.sort();
    assert_eq!(skus.len(), 2);
    assert_eq!(skus[0], "DUP-0001");
    assert!(skus[1].starts_with("DUP-0001-"), "{skus:?}");
    let conflicts = app
        .mongo()
        .collection::<Document>("sync_conflicts")
        .count_documents(doc! { "tenant_id": "tnt_u", "kind": "UNIQUE_VIOLATION" })
        .await
        .expect("the awaited step succeeds");
    assert_eq!(conflicts, 1);
    drop_db(&app).await;
}

// ---------------------------------------------------------------------------
// Quotations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pushed_quotations_apply_merge_forward_and_are_never_reopened() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;

    // A genuine wire payload: a stored document run through the same hydrate a pull uses.
    let made = web_quotation(&app, "tnt_qsrc", "Nimal").await;
    let raw = stored_quotation(&app, "tnt_qsrc", &text(&made, "/key")).await;
    let payload = with_tenant(
        Tenant::id("tnt_qsrc").expect("the tenant id is valid"),
        hydrate_for_test(&app.db_handle, "quotations", raw),
    )
    .await
    .expect("the awaited step succeeds");
    assert!(payload["shopProfileSnapshot"].is_object(), "{payload}");
    assert!(payload.get("isExpired").is_none(), "{payload}");
    let payload = reid(&payload);
    let key = text(&payload, "/key");

    // 1. A new quotation applies, with its frozen shop profile and number intact.
    let (_, body) = push(
        &app,
        "tnt_qd",
        "dev_q1",
        "b1",
        vec![record("quotations", &payload, "dev_q1")],
    )
    .await;
    assert_eq!(statuses(&body), ["applied"], "{body}");
    let stored = stored_quotation(&app, "tnt_qd", &key).await;
    assert_eq!(
        stored
            .get_str("status")
            .expect("the document has this string field"),
        "active"
    );
    assert_eq!(
        stored
            .get_str("quotation_number")
            .expect("the document has this string field"),
        payload["quotationNumber"]
            .as_str()
            .expect("the value is a string")
    );
    assert!(stored.get_document("shop_profile_snapshot").is_ok());
    assert_eq!(
        stored
            .get_array("items")
            .expect("the document has this array field")
            .len(),
        1
    );

    // 2. Its cancellation (a later version of the same row) moves it forward.
    let mut cancelled = payload.clone();
    cancelled["status"] = json!("cancelled");
    cancelled["version"] = json!(2);
    cancelled["updatedAt"] = json!(chrono::Utc::now().to_rfc3339());
    let (_, body) = push(
        &app,
        "tnt_qd",
        "dev_q1",
        "b2",
        vec![record("quotations", &cancelled, "dev_q1")],
    )
    .await;
    assert_eq!(statuses(&body), ["applied"], "{body}");
    let stored = stored_quotation(&app, "tnt_qd", &key).await;
    assert_eq!(
        stored
            .get_str("status")
            .expect("the document has this string field"),
        "cancelled"
    );
    assert_eq!(
        stored
            .get_i64("version")
            .expect("the document has this field"),
        2
    );

    // 3. A stale "active" copy arriving afterwards (a slow device) cannot reopen it.
    let (_, body) = push(
        &app,
        "tnt_qd",
        "dev_q2",
        "b3",
        vec![record("quotations", &payload, "dev_q2")],
    )
    .await;
    assert!(
        !statuses(&body).contains(&"rejected".to_string()),
        "a stale copy is ignored, not an error: {body}"
    );
    assert_eq!(
        stored_quotation(&app, "tnt_qd", &key)
            .await
            .get_str("status")
            .expect("the document has this string field"),
        "cancelled"
    );

    // 4. The same key with different content is refused, never silently adopted.
    let mut tampered = cancelled.clone();
    tampered["totalCents"] = json!(1);
    tampered["updatedAt"] = json!(chrono::Utc::now().to_rfc3339());
    let (_, body) = push(
        &app,
        "tnt_qd",
        "dev_q3",
        "b4",
        vec![record("quotations", &tampered, "dev_q3")],
    )
    .await;
    assert_eq!(statuses(&body), ["rejected"], "{body}");
    assert_ne!(
        stored_quotation(&app, "tnt_qd", &key)
            .await
            .get_i64("total_cents")
            .expect("the document has this field"),
        1
    );

    // 5. Two quotations that were given the same number: the second is renumbered and reported.
    let mut twin = reid(&payload);
    // The lower key wins a number clash, and the original's key is random; an extension of it always
    // sorts after it, so the twin is the one renumbered on every run (a fixed key lost 1 run in 10).
    let twin_key = format!("{key}_twin");
    twin["key"] = json!(twin_key);
    twin["version"] = json!(1);
    twin["status"] = json!("active");
    let (_, body) = push(
        &app,
        "tnt_qd",
        "dev_q1",
        "b5",
        vec![record("quotations", &twin, "dev_q1")],
    )
    .await;
    assert_eq!(statuses(&body), ["applied"], "{body}");
    let original_number = payload["quotationNumber"]
        .as_str()
        .expect("the value is a string");
    let renamed = stored_quotation(&app, "tnt_qd", &twin_key).await;
    let renamed_number = renamed
        .get_str("quotation_number")
        .expect("the document has this string field");
    assert!(
        renamed_number.starts_with(&format!("{original_number}-")),
        "expected a device-tagged number, got {renamed_number}"
    );
    let conflicts = app
        .mongo()
        .collection::<Document>("sync_conflicts")
        .count_documents(doc! { "tenant_id": "tnt_qd", "kind": "UNIQUE_VIOLATION" })
        .await
        .expect("the awaited step succeeds");
    assert_eq!(conflicts, 1);
    drop_db(&app).await;
}

#[tokio::test]
async fn a_web_quotation_and_its_cancellation_reach_devices() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;

    let made = web_quotation(&app, "tnt_wq", "Web customer").await;
    let key = text(&made, "/key");
    let dev = device_token(&app, "tnt_wq", "dev_wq1").await;

    let feed = wait_for(&app, &dev, 0, |c| {
        c.iter()
            .any(|x| x["resource"] == "quotations" && x["key"] == key)
    })
    .await;
    let change = feed["changes"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| c["resource"] == "quotations" && c["key"] == key)
        .expect("a match exists");
    // A web write has no device: every device receives it, in full wire form.
    assert_eq!(change["originDeviceId"], "cloud", "{change}");
    assert_eq!(change["payload"]["status"], "active");
    assert_eq!(
        change["payload"]["quotationNumber"],
        made["quotationNumber"]
    );
    assert!(change["payload"]["shopProfileSnapshot"].is_object());
    assert!(change["payload"].get("isExpired").is_none());

    web_cancel_quotation(&app, "tnt_wq", &key).await;
    let feed = wait_for(&app, &dev, 0, |c| {
        c.iter().any(|x| {
            x["resource"] == "quotations"
                && x["key"] == key
                && x["payload"]["status"] == "cancelled"
        })
    })
    .await;
    let cancelled = feed["changes"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| {
            c["resource"] == "quotations"
                && c["key"] == key
                && c["payload"]["status"] == "cancelled"
        })
        .expect("a match exists");
    assert_eq!(cancelled["payload"]["version"], 2);
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
    let dev = device_token(&app, "tnt_c", "dev_c1").await;
    let feed = wait_for(&app, &dev, 0, |c| c.len() >= 5).await;
    let newest = feed["nextSeq"].as_i64().expect("the value is an integer");
    assert!(newest >= 5);

    let dropped = with_tenant(
        Tenant::id("tnt_c").expect("the tenant id is valid"),
        compact_tenant(&app.db_handle, 0),
    )
    .await
    .expect("the awaited step succeeds");
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
        .expect("the value is an array")
        .iter()
        .map(|c| {
            c["key"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert!(keys.contains(&text(&seed.product, "/key")));
    assert!(keys.contains(&text(&seed.customer, "/key")));
    let as_of = snap["data"]["asOfSeq"]
        .as_i64()
        .expect("the value is an integer");
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
    web_quotation(&app, "tnt_p", "Snapshot customer").await;
    let dev = device_token(&app, "tnt_p", "dev_p1").await;

    let mut expected = 0u64;
    for spec in simplebash_pos_backend::modules::sync::resources::SYNC_RESOURCES
        .iter()
        .filter(|s| s.phase == simplebash_pos_backend::modules::sync::resources::Phase::P3a)
    {
        expected += app
            .mongo()
            .collection::<Document>(spec.table)
            .count_documents(doc! { "tenant_id": "tnt_p", "deleted_at": Bson::Null })
            .await
            .expect("the awaited step succeeds");
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
        let seq = data["asOfSeq"].as_i64().expect("the value is an integer");
        assert_eq!(
            *as_of.get_or_insert(seq),
            seq,
            "asOfSeq must not move between pages"
        );
        for c in data["changes"].as_array().expect("the value is an array") {
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
    use argon2::Argon2;
    use argon2::password_hash::rand_core::OsRng;
    use argon2::password_hash::{PasswordHasher, SaltString};
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;

    let shop = create_tenant(&app.db_handle, "shop-cashier", "Cashier Shop")
        .await
        .expect("the awaited step succeeds");
    let other = create_tenant(&app.db_handle, "shop-other", "Other Shop")
        .await
        .expect("the awaited step succeeds");
    let tid = shop.key.clone();
    with_tenant(
        Tenant::id(&tid).expect("valid tenant id"),
        ensure_shop_profile(&app.db_handle, Some("Cashier Shop")),
    )
    .await
    .expect("shop profile initializes");

    let hash = Argon2::default()
        .hash_password(b"cashier-pass-9", &SaltString::generate(&mut OsRng))
        .expect("the password hashes")
        .to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let id = ObjectId::new().to_hex();
    let payload = json!({
        "id": id, "key": "usr_cashier_cloud", "name": "Cloud Cashier", "username": "cashier",
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
    let login = |shop_code: &str, password: &str| json!({ "username": "cashier", "password": password, "shopCode": shop_code });
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
    let observer = device_token(&app, &tid, "dev_u2").await;
    let feed = wait_for(&app, &observer, 0, |changes| {
        changes
            .iter()
            .any(|c| c["resource"] == "users" && c["key"] == "usr_cashier_cloud")
    })
    .await;
    let pulled = feed["changes"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|c| {
            c["resource"] == "users" && c["key"] == "usr_cashier_cloud" && c["op"] == "upsert"
        })
        .expect("the cashier is in the change feed");
    assert_eq!(pulled["payload"]["passwordHash"], hash);
    assert_eq!(pulled["payload"]["id"], id);
    let cursor = feed["nextSeq"].as_i64().expect("the value is an integer");

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
    // The device that pushed the cashier learns of the web delete as well: the
    // web write names its own origin instead of keeping the pusher's.
    let pusher = device_token(&app, &tid, "dev_u1").await;
    wait_for(&app, &pusher, cursor, |changes| {
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

    app.mongo().drop().await.ok();
}
