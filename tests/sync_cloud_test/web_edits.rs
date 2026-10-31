//! Web edits tests.

use axum::body::Body;
use axum::http::header::AUTHORIZATION;
use axum::http::{Request, StatusCode};

use mongodb::bson::doc;
use serde_json::{Value, json};

use tower::ServiceExt;

use crate::common;
use crate::helpers::{
    SERIAL, admin_token, call, device_token, drop_db, push, record, reid, seed_tenant, start,
    statuses, text, wait_for,
};

/// Reads the event stream until `needle` appears; `false` on timeout or end.
async fn read_until(
    body: &mut axum::body::BodyDataStream,
    seen: &mut String,
    needle: &str,
    limit: std::time::Duration,
) -> bool {
    use futures_util::StreamExt;
    let deadline = std::time::Instant::now() + limit;
    while !seen.contains(needle) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(left, body.next()).await {
            Ok(Some(Ok(chunk))) => seen.push_str(&String::from_utf8_lossy(&chunk)),
            _ => return false,
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Realtime sync: every write reaches every other replica, fast
// ---------------------------------------------------------------------------

/// A pushed row that is later edited on the web must reach the device that
/// pushed it. Before writes were stamped centrally, a web edit kept the
/// pusher's `updated_by_device`, so that device's pull skipped it as its own.
#[tokio::test]
async fn a_web_edit_reaches_the_device_that_last_wrote_the_row() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let seed = seed_tenant(&app, "tnt_src").await;

    let tid = "tnt_edit";
    let customer = reid(&seed.customer);
    let product = reid(&seed.product);
    let category = seed.category.clone();
    let (status, pushed) = push(
        &app,
        tid,
        "dev_e1",
        "batch-e1",
        vec![
            record("categories", &category, "dev_e1"),
            record("customers", &customer, "dev_e1"),
            record("products", &product, "dev_e1"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "push: {pushed}");
    assert!(statuses(&pushed).iter().all(|s| s == "applied"), "{pushed}");

    let dev = device_token(&app, tid, "dev_e1").await;
    let start_feed = wait_for(&app, &dev, 0, |_| true).await;
    let cursor = start_feed["nextSeq"]
        .as_i64()
        .expect("the value is an integer");

    let admin = admin_token(&app, tid);
    let (status, body) = call(
        &app.router,
        "PATCH",
        &format!("/api/customers/{}", text(&customer, "/id")),
        Some(&admin),
        Some(json!({ "name": "Edited On The Web" })),
    )
    .await;
    assert!(status.is_success(), "customer edit: {status} {body}");
    let (status, body) = call(
        &app.router,
        "PUT",
        &format!("/api/inventory/products/{}", text(&product, "/id")),
        Some(&admin),
        Some(json!({ "minStockThreshold": 7 })),
    )
    .await;
    assert!(status.is_success(), "product edit: {status} {body}");

    let customer_key = text(&customer, "/key");
    let product_key = text(&product, "/key");
    let feed = wait_for(&app, &dev, cursor, |c| {
        c.iter().any(|x| x["key"] == customer_key.as_str())
            && c.iter().any(|x| x["key"] == product_key.as_str())
    })
    .await;
    let changes = feed["changes"].as_array().expect("the value is an array");
    let edited = changes
        .iter()
        .find(|c| c["key"] == customer_key.as_str())
        .expect("a match exists");
    assert_eq!(edited["payload"]["name"], "Edited On The Web");
    assert_eq!(edited["originDeviceId"], "cloud");
    let priced = changes
        .iter()
        .find(|c| c["key"] == product_key.as_str())
        .expect("a match exists");
    assert_eq!(priced["payload"]["minStockThreshold"], 7);
    drop_db(&app).await;
}

/// Removals the web makes outright, and subcategory edits (which travel inside
/// their category), reach devices.
#[tokio::test]
async fn web_removals_and_subcategory_edits_reach_devices() {
    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let tid = "tnt_rm";
    let seed = seed_tenant(&app, tid).await;
    let admin = admin_token(&app, tid);
    let dev = device_token(&app, tid, "dev_r1").await;
    let category_key = text(&seed.category, "/key");
    let initial = wait_for(&app, &dev, 0, |c| {
        c.iter().any(|x| x["key"] == category_key.as_str())
    })
    .await;
    let cursor = initial["nextSeq"]
        .as_i64()
        .expect("the value is an integer");

    // A new subcategory is a newer version of its category.
    let (status, body) = call(
        &app.router,
        "POST",
        &format!("/api/inventory/categories/{category_key}/subcategories"),
        Some(&admin),
        Some(json!({ "name": "Beta" })),
    )
    .await;
    assert!(status.is_success(), "add subcategory: {status} {body}");
    let feed = wait_for(&app, &dev, cursor, |c| {
        c.iter().any(|x| {
            x["key"] == category_key.as_str()
                && x["payload"]["subcategories"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|sub| sub["name"] == "Beta"))
        })
    })
    .await;
    let change = feed["changes"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|x| x["key"] == category_key.as_str())
        .expect("a match exists")
        .clone();
    assert!(
        change["version"].as_i64().expect("the value is an integer")
            > seed.category["version"]
                .as_i64()
                .expect("the value is an integer"),
        "the category version moved so devices apply it: {change}"
    );
    let cursor = feed["nextSeq"].as_i64().expect("the value is an integer");

    // A supplier link removed on the web is removed on devices.
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/supplier-products",
        Some(&admin),
        Some(json!({ "supplierKey": seed.supplier["key"], "productKey": seed.product["key"], "costPriceCents": 900 })),
    )
    .await;
    assert!(status.is_success(), "link: {status} {body}");
    let link_key = text(&body["data"], "/key");
    let (status, body) = call(
        &app.router,
        "DELETE",
        &format!(
            "/api/supplier-products/{}/{}",
            text(&seed.supplier, "/key"),
            text(&seed.product, "/key")
        ),
        Some(&admin),
        None,
    )
    .await;
    assert!(status.is_success(), "unlink: {status} {body}");

    // A category deleted on the web is deleted on devices.
    let (status, body) = call(
        &app.router,
        "POST",
        "/api/inventory/categories",
        Some(&admin),
        Some(json!({ "name": "Short Lived", "icon": "Box", "color": "red", "subcategories": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let doomed = text(&body["data"], "/key");
    let (status, body) = call(
        &app.router,
        "DELETE",
        &format!("/api/inventory/categories/{doomed}"),
        Some(&admin),
        None,
    )
    .await;
    assert!(status.is_success(), "category delete: {status} {body}");

    let deleted = |c: &Vec<Value>, resource: &str, key: &str| {
        c.iter()
            .any(|x| x["resource"] == resource && x["key"] == key && x["op"] == "delete")
    };
    wait_for(&app, &dev, cursor, |c| {
        deleted(c, "supplierProducts", &link_key) && deleted(c, "categories", &doomed)
    })
    .await;
    drop_db(&app).await;
}

/// `GET /api/sync/events`: a device hears about another writer's change
/// within moments and never about its own.
#[tokio::test]
async fn the_event_stream_announces_other_writers_changes_at_once() {
    use simplebash_pos_backend::modules::sync::live::run_watcher;
    use std::time::Duration;

    let _guard = SERIAL.lock().await;
    let app = common::spawn_app_multi_tenant().await;
    start(&app).await;
    let db = app.db_handle.clone();
    tokio::spawn(async move {
        run_watcher(db).await;
    });
    // Give the watcher's change stream a moment to open (it starts at "now").
    tokio::time::sleep(Duration::from_secs(3)).await;

    let tid = "tnt_live";
    let seed = seed_tenant(&app, "tnt_live_src").await;
    let listener = device_token(&app, tid, "dev_l1").await;
    let request = Request::builder()
        .method("GET")
        .uri("/api/sync/events")
        .header(AUTHORIZATION, format!("Bearer {listener}"))
        .body(Body::empty())
        .expect("the request is well formed");
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("the awaited step succeeds");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .expect("the header is text")
            .starts_with("text/event-stream")
    );
    let mut body = response.into_body().into_data_stream();
    let mut seen = String::new();
    assert!(
        read_until(&mut body, &mut seen, "event: hello", Duration::from_secs(5)).await,
        "hello first: {seen}"
    );

    // The listener's own push is not announced to it; another device's is.
    let own = push(
        &app,
        tid,
        "dev_l1",
        "batch-l1",
        vec![record("customers", &reid(&seed.customer), "dev_l1")],
    )
    .await;
    assert_eq!(own.0, StatusCode::OK, "{}", own.1);
    let started = std::time::Instant::now();
    let other = push(
        &app,
        tid,
        "dev_l2",
        "batch-l2",
        vec![record("suppliers", &reid(&seed.supplier), "dev_l2")],
    )
    .await;
    assert_eq!(other.0, StatusCode::OK, "{}", other.1);
    assert!(
        read_until(
            &mut body,
            &mut seen,
            "\"resource\":\"suppliers\"",
            Duration::from_secs(15)
        )
        .await,
        "no change event within 15 s: {seen}"
    );
    let latency = started.elapsed();
    println!("push -> event latency against Atlas: {latency:?}");
    assert!(
        !seen.contains("\"origin\":\"dev_l1\""),
        "the listener's own write was announced to it: {seen}"
    );
    assert!(seen.contains("event: change"), "{seen}");
    drop_db(&app).await;
}
