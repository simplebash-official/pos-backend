//! Local sync routes tests.

use axum::http::StatusCode;

use serde_json::json;

use sqlx::SqlitePool;

use crate::helpers::{Device, as_pulled, assert_same_data, shop};

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_sync_routes_need_the_service_token() {
    let dev = Device::new().await;

    let (status, _) = dev.call("GET", "/api/sync/state", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A normal admin token is not a sync service token.
    let (status, _) = dev
        .call("GET", "/api/sync/state", None, Some(&dev.admin.clone()))
        .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
        "{status}"
    );
    let (status, _) = dev
        .call(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "incremental", "changes": [] })),
            Some(&dev.admin.clone()),
        )
        .await;
    assert!(
        status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
        "{status}"
    );

    let state = dev.state().await;
    assert!(
        state["deviceId"]
            .as_str()
            .expect("the value is a string")
            .starts_with("dev_")
    );
    assert_eq!(state["linked"], false);
    assert_eq!(state["captureEnabled"], false);
    assert_eq!(state["localHasData"], false);
    assert_eq!(state["numberBlocks"], json!([]));
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

#[tokio::test]
async fn triggers_are_inert_until_enabled_then_enqueue_and_coalesce() {
    let dev = Device::new().await;
    dev.api(
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "Before", "icon": "x", "color": "red" })),
    )
    .await;
    assert_eq!(
        dev.pending_out().await,
        0,
        "capture is off: nothing is queued"
    );

    // Enabling seeds the outbox with what already exists.
    let enabled = dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    assert!(
        enabled["enqueued"]
            .as_i64()
            .expect("the value is an integer")
            >= 1
    );
    assert_eq!(dev.state().await["captureEnabled"], true);
    dev.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;
    assert_eq!(dev.pending_out().await, 0);

    let cat = dev
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "After", "icon": "x", "color": "red" })),
        )
        .await;
    let key = cat["data"]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    assert_eq!(dev.pending_out().await, 1);

    // Two more edits of the same row stay ONE outbox entry.
    for name in ["After 2", "After 3"] {
        dev.api(
            "PUT",
            &format!("/api/inventory/categories/{key}"),
            Some(json!({ "name": name })),
        )
        .await;
    }
    let items = dev.drain_outbox().await;
    assert_eq!(items.len(), 1, "coalesced per (resource, key)");
    assert_eq!(items[0]["record"]["key"], key);
    assert_eq!(items[0]["record"]["resource"], "categories");
    assert_eq!(items[0]["record"]["op"], "upsert");
    assert_eq!(items[0]["record"]["payload"]["name"], "After 3");
    assert!(
        items[0]["record"]["payload"]["id"].as_str().is_some(),
        "legacy id travels with the payload"
    );

    // Acknowledged rows are removed; new edits queue again with a higher seq.
    let last = items[0]["seq"].as_i64().expect("the value is an integer");
    dev.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": last })),
    )
    .await;
    assert_eq!(dev.pending_out().await, 0);
    dev.api("DELETE", &format!("/api/inventory/categories/{key}"), None)
        .await;
    let items = dev.drain_outbox().await;
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["record"]["op"], "delete",
        "soft delete is captured as a tombstone"
    );
    assert!(items[0]["seq"].as_i64().expect("the value is an integer") > last);
}

#[tokio::test]
async fn a_sale_moves_stock_without_resending_the_product_row() {
    let dev = Device::new().await;
    let keys = shop(&dev).await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    dev.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;

    dev.api(
        "POST",
        "/api/billing/sales",
        Some(json!({
            "staff": { "cashierName": "Admin" },
            "items": [{ "productKey": keys["productKey"], "sellingPriceCents": 50000, "quantity": 1, "discountCents": 0, "sourceType": "retail" }],
            "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 100000 },
            "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
        })),
    )
    .await;

    let resources: Vec<String> = dev
        .drain_outbox()
        .await
        .iter()
        .map(|i| {
            i["record"]["resource"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert!(
        resources.contains(&"stockMovements".to_string()),
        "{resources:?}"
    );
    assert!(resources.contains(&"invoices".to_string()), "{resources:?}");
    // The stock change travels as its ledger entry; re-sending the whole
    // product with a fresh timestamp would overwrite a newer price or name
    // edit made on another device.
    assert!(
        !resources.contains(&"products".to_string()),
        "{resources:?}"
    );
    // Same for the batch the units came out of: its remaining quantity is
    // derived from the same ledger.
    assert!(
        !resources.contains(&"stockBatches".to_string()),
        "{resources:?}"
    );

    // A real edit of the product is still captured.
    dev.api(
        "PUT",
        &format!(
            "/api/inventory/products/{}",
            keys["productId"].as_str().expect("the value is a string")
        ),
        Some(json!({ "minStockThreshold": 3 })),
    )
    .await;
    let resources: Vec<String> = dev
        .drain_outbox()
        .await
        .iter()
        .map(|i| {
            i["record"]["resource"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert!(resources.contains(&"products".to_string()), "{resources:?}");

    // And so is re-pricing a batch.
    let batches = dev
        .api(
            "GET",
            &format!(
                "/api/inventory/products/{}/batches",
                keys["productKey"].as_str().expect("the value is a string")
            ),
            None,
        )
        .await;
    let batch_key = batches["data"]["items"][0]["key"]
        .as_str()
        .expect("the value is a string");
    dev.api(
        "PATCH",
        &format!("/api/inventory/batches/{batch_key}"),
        Some(json!({ "sellingPriceCents": 51000 })),
    )
    .await;
    let resources: Vec<String> = dev
        .drain_outbox()
        .await
        .iter()
        .map(|i| {
            i["record"]["resource"]
                .as_str()
                .expect("the value is a string")
                .to_string()
        })
        .collect();
    assert!(
        resources.contains(&"stockBatches".to_string()),
        "{resources:?}"
    );
}

#[tokio::test]
async fn the_outbox_names_its_epoch() {
    let dev = Device::new().await;
    let page = dev
        .sync("GET", "/api/sync/outbox?after=0&limit=1", None)
        .await;
    let epoch = page["epoch"].as_str().expect("the value is a string");
    assert!(epoch.starts_with("ep_"), "{epoch}");
    // Stable for the life of the database.
    let again = dev
        .sync("GET", "/api/sync/outbox?after=0&limit=1", None)
        .await;
    assert_eq!(again["epoch"], page["epoch"]);
}

#[tokio::test]
async fn waiting_changes_are_counted_per_resource_and_named_for_people() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    dev.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;
    let empty = dev.sync("GET", "/api/sync/outbox/pending", None).await;
    assert_eq!(empty["total"], 0);
    assert_eq!(dev.state().await["pendingByResource"], json!([]));

    for name in ["Screens", "Cables"] {
        dev.api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": name, "icon": "x", "color": "red" })),
        )
        .await;
    }
    let state = dev.state().await;
    assert_eq!(
        state["pendingByResource"],
        json!([{ "resource": "categories", "count": 2 }])
    );
    assert_eq!(state["conflictsByResource"], json!([]));

    let all = dev.sync("GET", "/api/sync/outbox/pending", None).await;
    assert_eq!(all["total"], 2);
    let labels: Vec<&str> = all["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .map(|i| i["label"].as_str().expect("the value is a string"))
        .collect();
    assert_eq!(labels, vec!["Cables", "Screens"], "newest change first");
    assert_eq!(all["items"][0]["op"], "upsert");

    let capped = dev
        .sync(
            "GET",
            "/api/sync/outbox/pending?resource=categories&limit=1",
            None,
        )
        .await;
    assert_eq!(capped["total"], 2);
    assert_eq!(
        capped["items"]
            .as_array()
            .expect("the value is an array")
            .len(),
        1
    );
    let other = dev
        .sync("GET", "/api/sync/outbox/pending?resource=invoices", None)
        .await;
    assert_eq!(other["total"], 0);

    // A row deleted for good has no name left to show, but is still listed.
    let key = capped["items"][0]["key"]
        .as_str()
        .expect("the value is a string")
        .to_string();
    sqlx::query("DELETE FROM categories WHERE key = ?")
        .bind(&key)
        .execute(dev.pool())
        .await
        .expect("the awaited step succeeds");
    let after = dev
        .sync("GET", "/api/sync/outbox/pending?resource=categories", None)
        .await;
    let gone = after["items"]
        .as_array()
        .expect("the value is an array")
        .iter()
        .find(|i| i["key"] == key.as_str())
        .expect("a match exists");
    assert!(gone["label"].is_null());
}

#[tokio::test]
async fn nothing_is_captured_while_a_batch_is_being_applied() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    // Enabling queues what the device already holds (the built-in cash categories); only
    // the write made while a batch is being applied must add nothing.
    let queued_on_enable = dev.pending_out().await;

    sqlx::query("UPDATE sync_state SET applying = 1 WHERE id = 1")
        .execute(dev.pool())
        .await
        .expect("the awaited step succeeds");
    dev.api(
        "POST",
        "/api/inventory/categories",
        Some(json!({ "name": "During apply", "icon": "x", "color": "red" })),
    )
    .await;
    sqlx::query("UPDATE sync_state SET applying = 0 WHERE id = 1")
        .execute(dev.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(dev.pending_out().await, queued_on_enable);
}

#[tokio::test]
async fn opening_balance_migration_turns_stock_into_a_ledger_and_is_idempotent() {
    let dev = Device::new().await;
    // A legacy row: stock with no movement behind it.
    sqlx::query(
        "INSERT INTO products (key, id, sku, name, category_key, subcategory_key, \
         stock_quantity, min_stock_threshold, version, created_at, updated_at) \
         VALUES ('prod_legacy', '64b000000000000000000001', 'LEG-001', 'Legacy', 'cat_x', 'sub_x', 7, 0, 1, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .execute(dev.pool())
    .await
    .expect("the awaited step succeeds");

    let first = simplebash_pos_backend::modules::sync::state::migrate_opening_balances(dev.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(first, 1);
    let ledger: i64 = sqlx::query_scalar("SELECT SUM(quantity_delta) FROM stock_movements WHERE product_id = '64b000000000000000000001'")
        .fetch_one(dev.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(ledger, 7);
    // Stock from before the ledger is the product's opening batch.
    let batch_key: String = sqlx::query_scalar(
        "SELECT batch_key FROM stock_movements WHERE key = 'sm_open_prod_legacy'",
    )
    .fetch_one(dev.pool())
    .await
    .expect("the awaited step succeeds");
    assert_eq!(batch_key, "stb_open_prod_legacy");
    let kind: String = sqlx::query_scalar(
        "SELECT movement_type FROM stock_movements WHERE key = 'sm_open_prod_legacy'",
    )
    .fetch_one(dev.pool())
    .await
    .expect("the awaited step succeeds");
    assert_eq!(kind, "opening_balance");

    let second = simplebash_pos_backend::modules::sync::state::migrate_opening_balances(dev.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(second, 0, "a second run creates nothing");
}

#[tokio::test]
async fn creating_and_editing_stock_always_leaves_a_movement_behind() {
    let dev = Device::new().await;
    let keys = shop(&dev).await;
    let id = keys["productId"].as_str().expect("the value is a string");

    let ledger = |pool: SqlitePool, id: String| async move {
        let stock: i64 = sqlx::query_scalar("SELECT stock_quantity FROM products WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .expect("the awaited step succeeds");
        let sum: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(quantity_delta), 0) FROM stock_movements WHERE product_id = ?",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("the awaited step succeeds");
        (stock, sum)
    };
    let (stock, sum) = ledger(dev.pool().clone(), id.to_string()).await;
    assert_eq!(
        stock, sum,
        "created + purchased + sold + returned all explained by movements"
    );

    // Bringing stock in records a movement (into a new batch) instead of overwriting.
    dev.api(
        "PATCH",
        &format!("/api/inventory/products/{id}/stock"),
        Some(json!({ "delta": 5, "unitCostCents": 30000, "sellingPriceCents": 50000 })),
    )
    .await;
    let (stock_after, sum_after) = ledger(dev.pool().clone(), id.to_string()).await;
    assert_eq!(stock_after, stock + 5);
    assert_eq!(stock_after, sum_after);
}

// ---------------------------------------------------------------------------
// Applying
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_device_reproduces_another_devices_data_without_echoing_it_back() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    b.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    b.sync(
        "POST",
        "/api/sync/outbox/ack",
        Some(json!({ "upToSeq": 1_000_000 })),
    )
    .await;

    shop(&a).await;
    let changes = as_pulled(&a.drain_outbox().await);
    assert!(
        changes.len() >= 10,
        "a whole shop was captured: {}",
        changes.len()
    );

    let applied = b
        .sync(
            "POST",
            "/api/sync/apply",
            Some(json!({ "mode": "incremental", "changes": changes, "advanceCursorTo": 42 })),
        )
        .await;
    assert_eq!(applied["conflicts"], 0, "{applied}");
    assert_eq!(applied["cursor"], 42);
    assert!(
        applied["applied"]
            .as_u64()
            .expect("the value is an unsigned integer")
            >= 10
    );

    assert_same_data(&a, &b).await;
    assert_eq!(
        b.pending_out().await,
        0,
        "applied changes never echo into the outbox"
    );
    assert_eq!(b.state().await["cloudCursor"], 42);

    // Stock, balances and refund totals are ledger truth on the receiver.
    let stock: i64 = sqlx::query_scalar("SELECT stock_quantity FROM products")
        .fetch_one(b.pool())
        .await
        .expect("the awaited step succeeds");
    let ledger: i64 = sqlx::query_scalar("SELECT SUM(quantity_delta) FROM stock_movements")
        .fetch_one(b.pool())
        .await
        .expect("the awaited step succeeds");
    assert_eq!(stock, ledger);
    let refunded: i64 = sqlx::query_scalar("SELECT refunded_cents FROM invoices")
        .fetch_one(b.pool())
        .await
        .expect("the awaited step succeeds");
    assert!(
        refunded > 0,
        "the credit note's refund was recomputed onto the invoice"
    );
}
