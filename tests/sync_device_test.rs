// Device-side sync (SQLite): capture triggers, outbox, the applier, derived
// recompute, the opening-balance ledger migration, bootstrap and number blocks.
// Two in-process devices talk to each other through the real local sync API,
// exactly the way the desktop shell's agent drives them.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    clients::sqlite::map_sqlite_row_to_document,
    core::constants::roles,
    domain::{sequences::ReserveSequenceRequest, sync_v2::DerivedScope, users::Role},
    modules::sync::derived_sqlite,
};
use sqlx::SqlitePool;
use tower::ServiceExt;

struct Device {
    app: common::TestApp,
    admin: String,
    agent: String,
}

impl Device {
    async fn new() -> Device {
        let app = common::spawn_app_sqlite().await;
        let admin = common::mint_token(&app.config, Some(Role::Admin), roles::default_permissions(Role::Admin));
        let agent = encode(
            &Header::default(),
            &json!({ "sub": "sync-agent", "scope": "sync", "exp": 9_999_999_999u64 }),
            &EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
        )
        .unwrap();
        Device { app, admin, agent }
    }

    fn pool(&self) -> &SqlitePool {
        self.app.db_handle.as_sqlite().expect("sqlite device")
    }

    async fn call(&self, method: &str, uri: &str, body: Option<Value>, token: Option<&str>) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri).header(CONTENT_TYPE, "application/json");
        if let Some(t) = token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {t}"));
        }
        let body = body.map(|b| Body::from(serde_json::to_vec(&b).unwrap())).unwrap_or_else(Body::empty);
        let response = self.app.router.clone().oneshot(builder.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// A normal authenticated API call (as the POS UI would make).
    async fn api(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, value) = self.call(method, uri, body, Some(&self.admin.clone())).await;
        assert!(status.is_success(), "{method} {uri} -> {status}: {value}");
        value
    }

    /// A call to the local sync API with the shell's service token.
    async fn sync(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, value) = self.call(method, uri, body, Some(&self.agent.clone())).await;
        assert!(status.is_success(), "{method} {uri} -> {status}: {value}");
        value["data"].clone()
    }

    async fn state(&self) -> Value {
        self.sync("GET", "/api/sync/state", None).await
    }

    async fn pending_out(&self) -> i64 {
        self.state().await["pendingOut"].as_i64().unwrap()
    }

    /// Every outbox record, paged like the agent does.
    async fn drain_outbox(&self) -> Vec<Value> {
        let mut after = 0i64;
        let mut items = Vec::new();
        loop {
            let page = self.sync("GET", &format!("/api/sync/outbox?after={after}&limit=500"), None).await;
            let batch = page["items"].as_array().unwrap().clone();
            if batch.is_empty() {
                break;
            }
            after = page["lastSeq"].as_i64().unwrap();
            items.extend(batch);
        }
        items
    }
}

/// Outbox items as the `changes` of an apply request (what a pull returns).
fn as_pulled(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut change = item["record"].clone();
            change["seq"] = json!(i as i64 + 1);
            change
        })
        .collect()
}

async fn shop(dev: &Device) -> Value {
    let cat = dev
        .api("POST", "/api/inventory/categories", Some(json!({ "name": "Electronics", "icon": "devices", "color": "blue" })))
        .await;
    let category_key = cat["data"]["key"].as_str().unwrap().to_string();
    let sub = dev
        .api("POST", &format!("/api/inventory/categories/{category_key}/subcategories"), Some(json!({ "name": "Phones" })))
        .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"].as_str().unwrap().to_string();

    let product = dev
        .api(
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Phone", "categoryKey": category_key, "subcategoryKey": subcategory_key,
                "sellingPriceCents": 50000, "costPriceCents": 30000, "stockQuantity": 10, "minStockThreshold": 2
            })),
        )
        .await;
    let product_key = product["data"]["key"].as_str().unwrap().to_string();
    let product_id = product["data"]["id"].as_str().unwrap().to_string();

    let customer = dev
        .api("POST", "/api/customers", Some(json!({ "name": "Jane Doe", "primaryPhone": "0771234567", "email": "jane@example.com" })))
        .await;
    let customer_key = customer["data"]["key"].as_str().unwrap().to_string();

    let supplier = dev
        .api(
            "POST",
            "/api/suppliers",
            Some(json!({ "name": "Distributor", "contactPerson": "Sam", "primaryPhone": "0112345678", "address": "Colombo", "suppliedCategories": [category_key] })),
        )
        .await;
    let supplier_key = supplier["data"]["key"].as_str().unwrap().to_string();
    dev.api(
        "POST",
        "/api/supplier-products",
        Some(json!({ "supplierKey": supplier_key, "productKey": product_key, "costPriceCents": 29000 })),
    )
    .await;
    dev.api(
        "POST",
        "/api/purchases",
        Some(json!({ "supplierKey": supplier_key, "productKey": product_key, "quantity": 5, "unitCostCents": 29000, "date": "2026-09-06T00:00:00Z" })),
    )
    .await;

    let sale = dev
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "Admin" },
                "customer": { "customerKey": customer_key },
                "items": [{ "productKey": product_key, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 200000 },
                "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
            })),
        )
        .await;
    let invoice_key = sale["data"]["invoice"]["key"].as_str().unwrap().to_string();

    dev.api(
        "POST",
        "/api/billing/credit-notes",
        Some(json!({
            "invoiceKey": invoice_key,
            "returnedItems": [{ "productKey": product_key, "quantity": 1, "condition": "resalable", "reason": "defective" }],
            "paymentMethod": "cash"
        })),
    )
    .await;

    json!({
        "categoryKey": category_key, "subcategoryKey": subcategory_key, "productKey": product_key,
        "productId": product_id, "customerKey": customer_key, "supplierKey": supplier_key, "invoiceKey": invoice_key,
    })
}

/// Table contents with write-metadata removed, JSON columns parsed, for
/// comparing two devices row by row.
async fn table_rows(pool: &SqlitePool, table: &str) -> Vec<Value> {
    let rows = sqlx::query(&format!("SELECT * FROM {table} ORDER BY key")).fetch_all(pool).await.unwrap();
    rows.iter()
        .map(|row| {
            let mut value = serde_json::to_value(map_sqlite_row_to_document(row)).unwrap();
            let object = value.as_object_mut().unwrap();
            for meta in ["updated_at", "version", "updated_by_device"] {
                object.remove(meta);
            }
            // Subcategories have no legacy id on the wire (they are joined by key), so
            // each device mints its own.
            if table == "subcategories" {
                object.remove("id");
                object.remove("_id");
            }
            value
        })
        .collect()
}

const SYNCED: &[&str] = &[
    "categories", "subcategories", "suppliers", "products", "supplier_products", "customers", "purchases",
    "stock_movements", "invoices", "payments", "credit_notes",
];

async fn assert_same_data(a: &Device, b: &Device) {
    for table in SYNCED {
        assert_eq!(table_rows(a.pool(), table).await, table_rows(b.pool(), table).await, "table {table} differs");
    }
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn local_sync_routes_need_the_service_token() {
    let dev = Device::new().await;

    let (status, _) = dev.call("GET", "/api/sync/state", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // A normal admin token is not a sync service token.
    let (status, _) = dev.call("GET", "/api/sync/state", None, Some(&dev.admin.clone())).await;
    assert!(status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED, "{status}");
    let (status, _) = dev.call("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": [] })), Some(&dev.admin.clone())).await;
    assert!(status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED, "{status}");

    let state = dev.state().await;
    assert!(state["deviceId"].as_str().unwrap().starts_with("dev_"));
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
    dev.api("POST", "/api/inventory/categories", Some(json!({ "name": "Before", "icon": "x", "color": "red" }))).await;
    assert_eq!(dev.pending_out().await, 0, "capture is off: nothing is queued");

    // Enabling seeds the outbox with what already exists.
    let enabled = dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    assert!(enabled["enqueued"].as_i64().unwrap() >= 1);
    assert_eq!(dev.state().await["captureEnabled"], true);
    dev.sync("POST", "/api/sync/outbox/ack", Some(json!({ "upToSeq": 1_000_000 }))).await;
    assert_eq!(dev.pending_out().await, 0);

    let cat = dev.api("POST", "/api/inventory/categories", Some(json!({ "name": "After", "icon": "x", "color": "red" }))).await;
    let key = cat["data"]["key"].as_str().unwrap().to_string();
    assert_eq!(dev.pending_out().await, 1);

    // Two more edits of the same row stay ONE outbox entry.
    for name in ["After 2", "After 3"] {
        dev.api("PUT", &format!("/api/inventory/categories/{key}"), Some(json!({ "name": name }))).await;
    }
    let items = dev.drain_outbox().await;
    assert_eq!(items.len(), 1, "coalesced per (resource, key)");
    assert_eq!(items[0]["record"]["key"], key);
    assert_eq!(items[0]["record"]["resource"], "categories");
    assert_eq!(items[0]["record"]["op"], "upsert");
    assert_eq!(items[0]["record"]["payload"]["name"], "After 3");
    assert!(items[0]["record"]["payload"]["id"].as_str().is_some(), "legacy id travels with the payload");

    // Acknowledged rows are removed; new edits queue again with a higher seq.
    let last = items[0]["seq"].as_i64().unwrap();
    dev.sync("POST", "/api/sync/outbox/ack", Some(json!({ "upToSeq": last }))).await;
    assert_eq!(dev.pending_out().await, 0);
    dev.api("DELETE", &format!("/api/inventory/categories/{key}"), None).await;
    let items = dev.drain_outbox().await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["record"]["op"], "delete", "soft delete is captured as a tombstone");
    assert!(items[0]["seq"].as_i64().unwrap() > last);
}

#[tokio::test]
async fn nothing_is_captured_while_a_batch_is_being_applied() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;

    sqlx::query("UPDATE sync_state SET applying = 1 WHERE id = 1").execute(dev.pool()).await.unwrap();
    dev.api("POST", "/api/inventory/categories", Some(json!({ "name": "During apply", "icon": "x", "color": "red" }))).await;
    sqlx::query("UPDATE sync_state SET applying = 0 WHERE id = 1").execute(dev.pool()).await.unwrap();
    assert_eq!(dev.pending_out().await, 0);
}

#[tokio::test]
async fn opening_balance_migration_turns_stock_into_a_ledger_and_is_idempotent() {
    let dev = Device::new().await;
    // A legacy row: stock with no movement behind it.
    sqlx::query(
        "INSERT INTO products (key, id, sku, name, category_key, subcategory_key, cost_price_cents, selling_price_cents, \
         stock_quantity, min_stock_threshold, version, created_at, updated_at) \
         VALUES ('prod_legacy', '64b000000000000000000001', 'LEG-001', 'Legacy', 'cat_x', 'sub_x', 1, 2, 7, 0, 1, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .execute(dev.pool())
    .await
    .unwrap();

    let first = simplebash_pos_backend::modules::sync::state::migrate_opening_balances(dev.pool()).await.unwrap();
    assert_eq!(first, 1);
    let ledger: i64 = sqlx::query_scalar("SELECT SUM(quantity_delta) FROM stock_movements WHERE product_id = '64b000000000000000000001'")
        .fetch_one(dev.pool())
        .await
        .unwrap();
    assert_eq!(ledger, 7);
    let kind: String = sqlx::query_scalar("SELECT movement_type FROM stock_movements WHERE key = 'sm_open_prod_legacy'")
        .fetch_one(dev.pool())
        .await
        .unwrap();
    assert_eq!(kind, "opening_balance");

    let second = simplebash_pos_backend::modules::sync::state::migrate_opening_balances(dev.pool()).await.unwrap();
    assert_eq!(second, 0, "a second run creates nothing");
}

#[tokio::test]
async fn creating_and_editing_stock_always_leaves_a_movement_behind() {
    let dev = Device::new().await;
    let keys = shop(&dev).await;
    let id = keys["productId"].as_str().unwrap();

    let ledger = |pool: SqlitePool, id: String| async move {
        let stock: i64 = sqlx::query_scalar("SELECT stock_quantity FROM products WHERE id = ?").bind(&id).fetch_one(&pool).await.unwrap();
        let sum: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(quantity_delta), 0) FROM stock_movements WHERE product_id = ?").bind(&id).fetch_one(&pool).await.unwrap();
        (stock, sum)
    };
    let (stock, sum) = ledger(dev.pool().clone(), id.to_string()).await;
    assert_eq!(stock, sum, "created + purchased + sold + returned all explained by movements");

    // Editing the quantity on the product records a movement instead of overwriting.
    dev.api("PUT", &format!("/api/inventory/products/{id}"), Some(json!({ "stockQuantity": stock + 5 }))).await;
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
    b.sync("POST", "/api/sync/outbox/ack", Some(json!({ "upToSeq": 1_000_000 }))).await;

    shop(&a).await;
    let changes = as_pulled(&a.drain_outbox().await);
    assert!(changes.len() >= 10, "a whole shop was captured: {}", changes.len());

    let applied = b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": changes, "advanceCursorTo": 42 }))).await;
    assert_eq!(applied["conflicts"], 0, "{applied}");
    assert_eq!(applied["cursor"], 42);
    assert!(applied["applied"].as_u64().unwrap() >= 10);

    assert_same_data(&a, &b).await;
    assert_eq!(b.pending_out().await, 0, "applied changes never echo into the outbox");
    assert_eq!(b.state().await["cloudCursor"], 42);

    // Stock, balances and refund totals are ledger truth on the receiver.
    let stock: i64 = sqlx::query_scalar("SELECT stock_quantity FROM products").fetch_one(b.pool()).await.unwrap();
    let ledger: i64 = sqlx::query_scalar("SELECT SUM(quantity_delta) FROM stock_movements").fetch_one(b.pool()).await.unwrap();
    assert_eq!(stock, ledger);
    let refunded: i64 = sqlx::query_scalar("SELECT refunded_cents FROM invoices").fetch_one(b.pool()).await.unwrap();
    assert!(refunded > 0, "the credit note's refund was recomputed onto the invoice");
}

#[tokio::test]
async fn applying_the_same_batch_twice_changes_nothing() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    shop(&a).await;
    let changes = as_pulled(&a.drain_outbox().await);

    let body = json!({ "mode": "incremental", "changes": changes });
    let first = b.sync("POST", "/api/sync/apply", Some(body.clone())).await;
    let before: Vec<Vec<Value>> = {
        let mut v = Vec::new();
        for t in SYNCED {
            v.push(table_rows(b.pool(), t).await);
        }
        v
    };
    let second = b.sync("POST", "/api/sync/apply", Some(body)).await;
    assert_eq!(second["applied"], 0, "nothing new: {second}");
    assert_eq!(second["duplicates"], first["applied"], "every change is recognised as already applied");
    for (i, t) in SYNCED.iter().enumerate() {
        assert_eq!(before[i], table_rows(b.pool(), t).await, "table {t} changed on re-apply");
    }
}

#[tokio::test]
async fn derived_columns_are_never_taken_from_a_payload() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    shop(&a).await;
    let mut changes = as_pulled(&a.drain_outbox().await);

    for change in changes.iter_mut() {
        match change["resource"].as_str().unwrap() {
            "products" => change["payload"]["stockQuantity"] = json!(999),
            "customers" => {
                change["payload"]["totalPurchasesCents"] = json!(123_456);
                change["payload"]["outstandingBalanceCents"] = json!(654_321);
            }
            "invoices" => {
                change["payload"]["refundedCents"] = json!(777);
                change["payload"]["creditNoteCount"] = json!(9);
            }
            _ => {}
        }
    }
    b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": changes }))).await;
    assert_same_data(&a, &b).await;
}

#[tokio::test]
async fn recompute_restores_customer_balances_and_invoice_totals_from_the_ledger() {
    let dev = Device::new().await;
    let keys = shop(&dev).await;

    let purchases: i64 = sqlx::query_scalar("SELECT total_purchases_cents FROM customers").fetch_one(dev.pool()).await.unwrap();
    let refunded: i64 = sqlx::query_scalar("SELECT refunded_cents FROM invoices").fetch_one(dev.pool()).await.unwrap();

    sqlx::query("UPDATE customers SET total_purchases_cents = 1, outstanding_balance_cents = 2").execute(dev.pool()).await.unwrap();
    sqlx::query("UPDATE invoices SET refunded_cents = 0, credit_note_count = 0").execute(dev.pool()).await.unwrap();
    sqlx::query("UPDATE products SET stock_quantity = 0").execute(dev.pool()).await.unwrap();

    let mut scope = DerivedScope::default();
    scope.customer_keys.insert(keys["customerKey"].as_str().unwrap().to_string());
    scope.invoice_keys.insert(keys["invoiceKey"].as_str().unwrap().to_string());
    scope.product_ids.insert(keys["productId"].as_str().unwrap().to_string());
    let mut conn = dev.pool().acquire().await.unwrap();
    let conflicts = derived_sqlite::recompute(&mut conn, &scope).await.unwrap();
    assert!(conflicts.is_empty(), "{conflicts:?}");
    drop(conn);

    let again: i64 = sqlx::query_scalar("SELECT total_purchases_cents FROM customers").fetch_one(dev.pool()).await.unwrap();
    assert_eq!(again, purchases);
    let outstanding: i64 = sqlx::query_scalar("SELECT outstanding_balance_cents FROM customers").fetch_one(dev.pool()).await.unwrap();
    assert_eq!(outstanding, 0, "a fully paid cash sale leaves no balance");
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT refunded_cents FROM invoices").fetch_one(dev.pool()).await.unwrap(), refunded);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT credit_note_count FROM invoices").fetch_one(dev.pool()).await.unwrap(), 1);
    let stock: i64 = sqlx::query_scalar("SELECT stock_quantity FROM products").fetch_one(dev.pool()).await.unwrap();
    let ledger: i64 = sqlx::query_scalar("SELECT SUM(quantity_delta) FROM stock_movements").fetch_one(dev.pool()).await.unwrap();
    assert_eq!(stock, ledger);
}

#[tokio::test]
async fn a_newer_edit_wins_and_the_losing_edit_is_kept_for_review() {
    let a = Device::new().await;
    let b = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    b.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    let keys = shop(&a).await;
    b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": as_pulled(&a.drain_outbox().await) }))).await;
    a.sync("POST", "/api/sync/outbox/ack", Some(json!({ "upToSeq": 1_000_000 }))).await;
    b.sync("POST", "/api/sync/outbox/ack", Some(json!({ "upToSeq": 1_000_000 }))).await;

    // Both devices rename the same product; B's edit is later.
    let id = keys["productId"].as_str().unwrap();
    a.api("PUT", &format!("/api/inventory/products/{id}"), Some(json!({ "name": "Phone (A)" }))).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    b.api("PUT", &format!("/api/inventory/products/{id}"), Some(json!({ "name": "Phone (B)" }))).await;

    let from_a = as_pulled(&a.drain_outbox().await);
    let from_b = as_pulled(&b.drain_outbox().await);

    // A receives B's newer edit: B wins on both replicas regardless of order.
    a.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": from_b }))).await;
    let applied = b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": from_a }))).await;
    assert_eq!(applied["applied"], 0, "A's older edit lost on B: {applied}");
    for dev in [&a, &b] {
        let name: String = sqlx::query_scalar("SELECT name FROM products").fetch_one(dev.pool()).await.unwrap();
        assert_eq!(name, "Phone (B)");
    }

    // The loser is recorded, listed, and can be resolved.
    let conflicts = b.sync("GET", "/api/sync/conflicts", None).await;
    let conflict = conflicts.as_array().unwrap().iter().find(|c| c["kind"] == "LWW_LOSER").expect("LWW loser recorded");
    assert_eq!(conflict["detail"]["losingPayload"]["name"], "Phone (A)");
    assert_eq!(b.state().await["conflictsOpen"].as_i64().unwrap(), 1);
    let key = conflict["key"].as_str().unwrap();
    b.sync("POST", &format!("/api/sync/conflicts/{key}/resolve"), Some(json!({ "resolution": "kept newer edit" }))).await;
    assert_eq!(b.state().await["conflictsOpen"].as_i64().unwrap(), 0);
}

#[tokio::test]
async fn bootstrap_replaces_local_data_and_only_the_first_page_wipes() {
    let a = Device::new().await;
    a.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    shop(&a).await;
    let changes = as_pulled(&a.drain_outbox().await);
    let (first_page, second_page) = changes.split_at(changes.len() / 2);

    let b = Device::new().await;
    b.api("POST", "/api/inventory/categories", Some(json!({ "name": "Local demo data", "icon": "x", "color": "red" }))).await;

    b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "bootstrap", "changes": first_page }))).await;
    assert_eq!(b.state().await["bootstrapActive"], true);
    let demo: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories WHERE name = 'Local demo data'").fetch_one(b.pool()).await.unwrap();
    assert_eq!(demo, 0, "the first bootstrap page wiped the previous data");

    b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "bootstrap", "changes": second_page, "advanceCursorTo": 7 }))).await;
    let state = b.state().await;
    assert_eq!(state["bootstrapActive"], false);
    assert_eq!(state["cloudCursor"], 7);
    assert_same_data(&a, &b).await;
}

// ---------------------------------------------------------------------------
// Number blocks and backup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linked_devices_draw_numbers_from_blocks_then_a_unique_fallback() {
    use simplebash_pos_backend::modules::sequences::service::reserve_sequence;
    let dev = Device::new().await;
    let single = || ReserveSequenceRequest { block_size: Some(1), device_id: None };

    // Unlinked: the ordinary local counter.
    let plain = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single()).await.unwrap();
    assert_eq!((plain.prefix.as_str(), plain.start), ("INV-", 1));

    dev.sync("POST", "/api/sync/enable", Some(json!({ "deviceId": "dev_zzzz0000aaaa1111" }))).await;
    assert_eq!(dev.state().await["deviceId"], "dev_zzzz0000aaaa1111");
    dev.sync(
        "POST",
        "/api/sync/blocks",
        Some(json!({ "name": "invoice", "prefix": "INV-", "padding": 6, "start": 500, "end": 501, "expiresAt": "2099-01-01T00:00:00Z" })),
    )
    .await;
    let blocks = dev.state().await["numberBlocks"].clone();
    assert_eq!(blocks[0]["name"], "invoice");
    assert_eq!(blocks[0]["remaining"], 2);
    assert_eq!(blocks[0]["blockSize"], 2);

    let n1 = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single()).await.unwrap();
    let n2 = reserve_sequence(&dev.app.db_handle, "invoices".to_string(), single()).await.unwrap();
    assert_eq!((n1.prefix.as_str(), n1.start), ("INV-", 500));
    assert_eq!((n2.prefix.as_str(), n2.start), ("INV-", 501));
    assert_eq!(dev.state().await["numberBlocks"][0]["remaining"], 0);

    // Block exhausted: per-device fallback series, never colliding with cloud numbers.
    let f1 = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single()).await.unwrap();
    let f2 = reserve_sequence(&dev.app.db_handle, "invoice".to_string(), single()).await.unwrap();
    assert_eq!(f1.prefix, "INV-Dzzzz-");
    assert_eq!((f1.start, f2.start), (1, 2));
    let rendered = |r: &simplebash_pos_backend::domain::sequences::SequenceReservationResponse| format!("{}{:0w$}", r.prefix, r.start, w = r.padding);
    let all = [rendered(&n1), rendered(&n2), rendered(&f1), rendered(&f2)];
    let unique: std::collections::HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "{all:?}");
}

#[tokio::test]
async fn backups_never_carry_sync_bookkeeping() {
    let dev = Device::new().await;
    dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
    dev.api("POST", "/api/inventory/categories", Some(json!({ "name": "Backed up", "icon": "x", "color": "red" }))).await;

    let export = dev.api("POST", "/api/backup/export", Some(json!({}))).await;
    let tables = export["data"]["tables"].as_object().or_else(|| export["data"].as_object()).expect("export lists tables");
    assert!(tables.contains_key("categories"));
    assert!(tables.keys().all(|t| !t.starts_with("sync_")), "sync tables leaked into the backup: {:?}", tables.keys().collect::<Vec<_>>());
}

// ---------------------------------------------------------------------------
// Users (P3b): logins travel with the shop
// ---------------------------------------------------------------------------

/// Creates a staff login on `dev` and returns `(id, key)`.
async fn create_cashier(dev: &Device, email: &str, password: &str) -> (String, String) {
    let created = dev
        .api("POST", "/api/users", Some(json!({ "name": "Cashier One", "email": email, "password": password, "role": "staff" })))
        .await;
    (
        created["data"]["id"].as_str().unwrap().to_string(),
        created["data"]["key"].as_str().unwrap().to_string(),
    )
}

async fn login_status(dev: &Device, email: &str, password: &str) -> StatusCode {
    dev.call("POST", "/api/auth/login", Some(json!({ "email": email, "password": password })), None).await.0
}

/// Two enabled devices with empty outboxes.
async fn linked_pair() -> (Device, Device) {
    let a = Device::new().await;
    let b = Device::new().await;
    for dev in [&a, &b] {
        dev.sync("POST", "/api/sync/enable", Some(json!({}))).await;
        dev.sync("POST", "/api/sync/outbox/ack", Some(json!({ "upToSeq": 1_000_000 }))).await;
    }
    (a, b)
}

async fn hand_over(from: &Device, to: &Device) -> Value {
    let changes = as_pulled(&from.drain_outbox().await);
    from.sync("POST", "/api/sync/outbox/ack", Some(json!({ "upToSeq": 1_000_000 }))).await;
    to.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": changes }))).await
}

#[tokio::test]
async fn a_cashier_created_on_one_device_can_log_in_on_another() {
    let (a, b) = linked_pair().await;
    let (id, key) = create_cashier(&a, "cash1@shop.test", "cashier-pass-1").await;

    let changes = as_pulled(&a.drain_outbox().await);
    let record = changes.iter().find(|c| c["resource"] == "users" && c["key"] == key.as_str()).expect("the user is captured");
    assert!(
        record["payload"]["passwordHash"].as_str().unwrap().starts_with("$argon2"),
        "the sync record carries the hash so the login works elsewhere"
    );
    b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": changes }))).await;

    // Same row on both devices: same legacy id (login tokens carry it) and same hash.
    let row = |dev: &Device| {
        let pool = dev.pool().clone();
        let key = key.clone();
        async move {
            sqlx::query_as::<_, (String, String, String)>("SELECT id, password_hash, role FROM users WHERE key = ?")
                .bind(key)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let (row_a, row_b) = (row(&a).await, row(&b).await);
    assert_eq!(row_a, row_b);
    assert_eq!(row_b.0, id);

    assert_eq!(login_status(&b, "cash1@shop.test", "cashier-pass-1").await, StatusCode::OK);
    assert_eq!(login_status(&b, "cash1@shop.test", "wrong-password").await, StatusCode::UNAUTHORIZED);
    // Applying created no echo: B has nothing to send back.
    assert_eq!(b.pending_out().await, 0);
}

#[tokio::test]
async fn deactivating_and_deleting_a_cashier_propagates() {
    let (a, b) = linked_pair().await;
    let (id, key) = create_cashier(&a, "cash2@shop.test", "cashier-pass-2").await;
    hand_over(&a, &b).await;
    assert_eq!(login_status(&b, "cash2@shop.test", "cashier-pass-2").await, StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.api("PATCH", &format!("/api/users/{id}"), Some(json!({ "isActive": false }))).await;
    hand_over(&a, &b).await;
    let active: bool = sqlx::query_scalar("SELECT is_active FROM users WHERE key = ?").bind(&key).fetch_one(b.pool()).await.unwrap();
    assert!(!active, "the deactivation reached B");
    assert_ne!(login_status(&b, "cash2@shop.test", "cashier-pass-2").await, StatusCode::OK, "a deactivated cashier cannot log in");

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    a.api("DELETE", &format!("/api/users/{id}"), None).await;
    hand_over(&a, &b).await;
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE key = ?").bind(&key).fetch_one(b.pool()).await.unwrap();
    assert_eq!(remaining, 0, "the delete removed the login on B");
    assert_ne!(login_status(&b, "cash2@shop.test", "cashier-pass-2").await, StatusCode::OK);
}

#[tokio::test]
async fn the_password_hash_never_lands_in_a_conflict_record() {
    let (a, b) = linked_pair().await;
    let (id, key) = create_cashier(&a, "cash3@shop.test", "cashier-pass-3").await;
    hand_over(&a, &b).await;

    // Both devices rename the cashier; B's edit is later, so A's edit loses on B.
    a.api("PATCH", &format!("/api/users/{id}"), Some(json!({ "name": "Cashier (A)" }))).await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    b.api("PATCH", &format!("/api/users/{id}"), Some(json!({ "name": "Cashier (B)" }))).await;
    let from_a = as_pulled(&a.drain_outbox().await);
    let applied = b.sync("POST", "/api/sync/apply", Some(json!({ "mode": "incremental", "changes": from_a }))).await;
    assert_eq!(applied["applied"], 0, "{applied}");

    let conflicts = b.sync("GET", "/api/sync/conflicts", None).await;
    let conflict = conflicts
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "LWW_LOSER" && c["entityKey"] == key.as_str())
        .expect("the losing user edit is recorded");
    assert_eq!(conflict["detail"]["losingPayload"]["name"], "Cashier (A)");
    assert!(conflict["detail"]["losingPayload"].get("passwordHash").is_none(), "{conflict}");
    let stored: String = sqlx::query_scalar("SELECT detail FROM sync_conflicts").fetch_one(b.pool()).await.unwrap();
    assert!(!stored.contains("argon2") && !stored.contains("passwordHash"), "hash leaked into sync_conflicts: {stored}");
}

#[tokio::test]
async fn two_admins_from_two_devices_are_both_kept_and_reported() {
    let (a, b) = linked_pair().await;
    // Each device made its own shop owner offline.
    let mk = |dev: &Device, email: &'static str| {
        let dev_admin = dev.admin.clone();
        let router = dev.app.router.clone();
        async move {
            let request = Request::builder()
                .method("POST")
                .uri("/api/users")
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {dev_admin}"))
                .body(Body::from(json!({ "name": "Owner", "email": email, "password": "owner-pass-12", "role": "staff" }).to_string()))
                .unwrap();
            let response = router.oneshot(request).await.unwrap();
            assert!(response.status().is_success());
        }
    };
    mk(&a, "owner-a@shop.test").await;
    mk(&b, "owner-b@shop.test").await;
    // Promote both to admin directly (the service's one-admin rule guards the API path only).
    for dev in [&a, &b] {
        sqlx::query("UPDATE users SET role = 'admin'").execute(dev.pool()).await.unwrap();
    }
    hand_over(&a, &b).await;

    let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin' AND deleted_at IS NULL")
        .fetch_one(b.pool())
        .await
        .unwrap();
    assert_eq!(admins, 2, "neither owner was dropped");
    let conflicts = b.sync("GET", "/api/sync/conflicts", None).await;
    let raised = conflicts.as_array().unwrap().iter().filter(|c| c["detail"]["reason"] == "MULTIPLE_ADMINS").count();
    assert_eq!(raised, 1, "the shop owner is told once: {conflicts}");
}

#[test]
fn logging_redaction_masks_the_password_hash_under_both_spellings() {
    use simplebash_pos_backend::core::logging::redact::redact_value;
    let mut value = json!({ "changes": [{ "payload": { "passwordHash": "$argon2id$v=19$secret", "name": "A" } }], "password_hash": "$argon2id$other" });
    redact_value(&mut value);
    let text = value.to_string();
    assert!(!text.contains("argon2"), "{text}");
    assert!(text.contains("\"name\":\"A\""));
}
