// End-to-end convergence: three offline-capable SQLite devices (the real desktop
// backend router each) and one multi-tenant cloud (real router, real MongoDB
// Atlas in a throwaway database, real change-stream consumer) talk through the
// same HTTP contracts the desktop sync agent uses. A seeded scenario makes each
// device work offline (sales, price edits, stock edits, new customers), syncs a
// random subset per round, then everything is driven to quiescence. Afterwards
// every replica must hold the same data, stock must equal its movement ledger
// on every device, and the cloud must hold exactly the ledger the devices do.

mod common;

use axum::{
    body::Body,
    http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
};
use jsonwebtoken::{EncodingKey, Header, encode};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::{
    core::{constants::roles, tenancy::Tenant},
    domain::users::Role,
    modules::sync::cloud_capture::run_consumer,
};
use sqlx::SqlitePool;
use std::collections::BTreeSet;
use tower::ServiceExt;
use uuid::Uuid;

const TENANT: &str = "tnt_convergence";
const SEED: u64 = 0x5EED_C0DE;
const ROUNDS: usize = 4;
const OPS_PER_ROUND: usize = 3;

// ---------------------------------------------------------------------------
// Tiny deterministic RNG (the scenario must be reproducible from SEED alone)
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

// ---------------------------------------------------------------------------
// A device: the real desktop backend on in-memory SQLite
// ---------------------------------------------------------------------------

struct Device {
    app: common::TestApp,
    admin: String,
    agent: String,
    id: String,
    cloud_token: String,
    cursor: i64,
}

impl Device {
    async fn new(n: usize, cloud: &common::TestApp) -> Device {
        let app = common::spawn_app_sqlite().await;
        let admin = common::mint_token(
            &app.config,
            Some(Role::Admin),
            roles::default_permissions(Role::Admin),
        );
        let agent = encode(
            &Header::default(),
            &json!({ "sub": "sync-agent", "scope": "sync", "exp": 9_999_999_999u64 }),
            &EncodingKey::from_secret(app.config.jwt_secret.as_bytes()),
        )
        .unwrap();
        let id = format!("dev_conv{n}aaaa{n}bbbb");
        let cloud_token = common::mint_device_token(&cloud.config, TENANT, &id);
        Device {
            app,
            admin,
            agent,
            id,
            cloud_token,
            cursor: 0,
        }
    }

    fn pool(&self) -> &SqlitePool {
        self.app.db_handle.as_sqlite().expect("sqlite device")
    }

    async fn api(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, value) = call(&self.app.router, method, uri, &self.admin, body).await;
        assert!(
            status.is_success(),
            "[{}] {method} {uri} -> {status}: {value}",
            self.id
        );
        value
    }

    async fn sync(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, value) = call(&self.app.router, method, uri, &self.agent, body).await;
        assert!(
            status.is_success(),
            "[{}] {method} {uri} -> {status}: {value}",
            self.id
        );
        value["data"].clone()
    }
}

// ---------------------------------------------------------------------------
// The agent's contract, as test code
// ---------------------------------------------------------------------------

/// Push everything in the local outbox to the cloud, acking only after a 2xx.
async fn push_all(cloud: &common::TestApp, d: &Device) {
    loop {
        let page = d
            .sync("GET", "/api/sync/outbox?after=0&limit=200", None)
            .await;
        let items = page["items"].as_array().unwrap().clone();
        if items.is_empty() {
            return;
        }
        let changes: Vec<Value> = items.iter().map(|i| i["record"].clone()).collect();
        let seqs: Vec<i64> = items.iter().map(|i| i["seq"].as_i64().unwrap()).collect();
        let (status, body) = call(
            &cloud.router,
            "POST",
            "/api/sync/push",
            &d.cloud_token,
            Some(json!({
                "deviceId": d.id, "batchId": Uuid::new_v4().to_string(),
                "baseSeq": d.cursor, "changes": changes, "outboxSeqs": seqs
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "[{}] push: {body}", d.id);
        for ack in body["data"]["acks"].as_array().unwrap() {
            assert_ne!(
                ack["status"], "rejected",
                "[{}] a change was rejected: {ack}",
                d.id
            );
        }
        d.sync(
            "POST",
            "/api/sync/outbox/ack",
            Some(json!({ "upToSeq": page["lastSeq"] })),
        )
        .await;
    }
}

/// Pull everything newer than the device's cursor and apply it locally.
async fn pull_all(cloud: &common::TestApp, d: &mut Device) {
    loop {
        let (status, body) = call(
            &cloud.router,
            "GET",
            &format!("/api/sync/pull?since={}&limit=200", d.cursor),
            &d.cloud_token,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "[{}] pull: {body}", d.id);
        let data = &body["data"];
        let next = data["nextSeq"].as_i64().unwrap();
        let changes = data["changes"].clone();
        if changes.as_array().is_some_and(|c| !c.is_empty()) {
            d.sync(
                "POST",
                "/api/sync/apply",
                Some(json!({ "mode": "incremental", "changes": changes, "advanceCursorTo": next })),
            )
            .await;
        }
        d.cursor = next;
        if !data["hasMore"].as_bool().unwrap_or(false) {
            return;
        }
    }
}

/// Replace the device's data with the cloud snapshot (a new device joining).
async fn bootstrap(cloud: &common::TestApp, d: &mut Device) {
    let mut pages: Vec<Vec<Value>> = Vec::new();
    let mut page_token: Option<String> = None;
    let as_of;
    loop {
        let uri = match &page_token {
            Some(p) => format!("/api/sync/snapshot?limit=200&page={p}"),
            None => "/api/sync/snapshot?limit=200".to_string(),
        };
        let (status, body) = call(&cloud.router, "GET", &uri, &d.cloud_token, None).await;
        assert_eq!(status, StatusCode::OK, "[{}] snapshot: {body}", d.id);
        let data = &body["data"];
        pages.push(
            data["changes"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(|mut c| {
                    c["seq"] = json!(0);
                    c
                })
                .collect(),
        );
        match data["nextPage"].as_str() {
            Some(p) => page_token = Some(p.to_string()),
            None => {
                as_of = data["asOfSeq"].as_i64().unwrap();
                break;
            }
        }
    }
    let last = pages.len() - 1;
    for (i, page) in pages.into_iter().enumerate() {
        let mut body = json!({ "mode": "bootstrap", "changes": page });
        if i == last {
            body["advanceCursorTo"] = json!(as_of);
        }
        d.sync("POST", "/api/sync/apply", Some(body)).await;
    }
    d.cursor = as_of;
}

/// Reserve a block of invoice numbers from the cloud and hand it to the device.
async fn reserve_blocks(cloud: &common::TestApp, d: &Device) {
    let (status, body) = call(
        &cloud.router,
        "POST",
        "/api/sequences/invoice/reserve",
        &d.cloud_token,
        Some(json!({ "blockSize": 200, "deviceId": d.id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "[{}] reserve: {body}", d.id);
    let b = &body["data"];
    d.sync(
        "POST",
        "/api/sync/blocks",
        Some(json!({
            "name": "invoice", "prefix": b["prefix"], "padding": b["padding"],
            "start": b["start"], "end": b["end"], "expiresAt": b["expiresAt"]
        })),
    )
    .await;
}

/// Every change the cloud holds, read as a device that never wrote anything.
async fn cloud_changes(cloud: &common::TestApp) -> Vec<Value> {
    let observer = common::mint_device_token(&cloud.config, TENANT, "dev_observer");
    let mut since = 0i64;
    let mut all = Vec::new();
    loop {
        let (status, body) = call(
            &cloud.router,
            "GET",
            &format!("/api/sync/pull?since={since}&limit=500"),
            &observer,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "observer pull: {body}");
        let data = &body["data"];
        all.extend(data["changes"].as_array().cloned().unwrap_or_default());
        since = data["nextSeq"].as_i64().unwrap();
        if !data["hasMore"].as_bool().unwrap_or(false) {
            return all;
        }
    }
}

/// Conflicts of a given kind the cloud has recorded for `TENANT` (`cloud_capture`
/// and `push` write to `sync_conflicts` the same way the device applier does).
async fn cloud_conflicts(cloud: &common::TestApp, kind: &str) -> Vec<Document> {
    use futures_util::TryStreamExt;
    cloud
        .db
        .collection::<Document>("sync_conflicts")
        .find(doc! { "tenant_id": TENANT, "kind": kind })
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}

/// The change-stream consumer is asynchronous: wait until the cloud's log stops growing.
async fn wait_for_cloud_quiet(cloud: &common::TestApp) {
    let mut stable = 0;
    let mut last = usize::MAX;
    for _ in 0..90 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let n = cloud_changes(cloud).await.len();
        if n == last {
            stable += 1;
            if stable >= 3 {
                return;
            }
        } else {
            stable = 0;
            last = n;
        }
    }
    panic!("the cloud change log never settled");
}

/// Starts the consumer and waits until it is really capturing (a change stream
/// only sees writes made after it opened).
async fn start_consumer(cloud: &common::TestApp) {
    cloud
        .db
        .create_collection("zz_bootstrap")
        .await
        .expect("create bootstrap collection");
    let db = cloud.db_handle.clone();
    tokio::spawn(async move {
        run_consumer(db).await;
    });
    let admin = common::mint_token_for_tenant(
        &cloud.config,
        Some(Role::Admin),
        roles::default_permissions(Role::Admin),
        Some("tnt_canary"),
    );
    let observer = common::mint_device_token(&cloud.config, "tnt_canary", "dev_canary");
    for attempt in 0..60 {
        let (status, body) = call(
            &cloud.router,
            "POST",
            "/api/inventory/categories",
            &admin,
            Some(json!({ "name": format!("Canary-{attempt}"), "icon": "Box", "color": "blue", "subcategories": [] })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "canary: {body}");
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let (status, body) = call(
            &cloud.router,
            "GET",
            "/api/sync/pull?since=0",
            &observer,
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

// ---------------------------------------------------------------------------
// Shop data and random operations
// ---------------------------------------------------------------------------

struct Catalog {
    products: Vec<(String, String)>, // (key, legacy id)
}

async fn create_shop(d: &Device) -> Catalog {
    let cat = d
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Electronics", "icon": "devices", "color": "blue" })),
        )
        .await;
    let category_key = cat["data"]["key"].as_str().unwrap().to_string();
    let sub = d
        .api(
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": "Phones" })),
        )
        .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let mut products = Vec::new();
    for name in ["Phone", "Charger"] {
        let p = d
            .api(
                "POST",
                "/api/inventory/products",
                Some(json!({
                    "name": name, "categoryKey": category_key, "subcategoryKey": subcategory_key,
                    "sellingPriceCents": 50000, "costPriceCents": 30000, "stockQuantity": 500, "minStockThreshold": 2
                })),
            )
            .await;
        products.push((
            p["data"]["key"].as_str().unwrap().to_string(),
            p["data"]["id"].as_str().unwrap().to_string(),
        ));
    }
    d.api(
        "POST",
        "/api/customers",
        Some(json!({ "name": "Walk-in Regular", "primaryPhone": "0770000000" })),
    )
    .await;
    Catalog { products }
}

async fn random_op(d: &Device, catalog: &Catalog, rng: &mut Rng, step: usize) {
    let (key, id) = &catalog.products[rng.below(catalog.products.len())];
    match rng.below(5) {
        0 | 1 => {
            let quantity = 1 + rng.below(3);
            d.api(
                "POST",
                "/api/billing/sales",
                Some(json!({
                    "staff": { "cashierName": "Admin" },
                    "items": [{ "productKey": key, "quantity": quantity, "discountCents": 0, "sourceType": "retail" }],
                    "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 1_000_000 },
                    "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
                })),
            )
            .await;
        }
        2 => {
            let price = 40_000 + 100 * rng.below(200) as i64;
            d.api(
                "PUT",
                &format!("/api/inventory/products/{id}"),
                Some(json!({ "sellingPriceCents": price })),
            )
            .await;
        }
        3 => {
            // A new cashier login: every replica must end up with the same account.
            d.api(
                "POST",
                "/api/users",
                Some(json!({
                    "name": format!("Cashier {step}"), "email": format!("staff-{}-{step}@shop.test", d.id),
                    "password": format!("cashier-pass-{step}"), "role": "staff"
                })),
            )
            .await;
        }
        _ => {
            if rng.below(2) == 0 {
                let current = d
                    .api("GET", &format!("/api/inventory/products/{id}"), None)
                    .await["data"]["stockQuantity"]
                    .as_i64()
                    .unwrap();
                let delta = rng.below(7) as i64 - 3;
                d.api(
                    "PUT",
                    &format!("/api/inventory/products/{id}"),
                    Some(json!({ "stockQuantity": current + delta })),
                )
                .await;
            } else {
                d.api(
                    "POST",
                    "/api/customers",
                    Some(json!({ "name": format!("Customer {} {step}", d.id), "primaryPhone": format!("07{:08}", 1000 + step) })),
                )
                .await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

const SYNCED: &[&str] = &[
    "categories",
    "subcategories",
    "suppliers",
    "products",
    "supplier_products",
    "customers",
    "purchases",
    "stock_movements",
    "invoices",
    "payments",
    "credit_notes",
    "users",
];

async fn table_rows(pool: &SqlitePool, table: &str) -> Vec<Value> {
    use simplebash_pos_backend::clients::sqlite::map_sqlite_row_to_document;
    let rows = sqlx::query(&format!("SELECT * FROM {table} ORDER BY key"))
        .fetch_all(pool)
        .await
        .unwrap();
    rows.iter()
        .map(|row| {
            let mut value = serde_json::to_value(map_sqlite_row_to_document(row)).unwrap();
            let object = value.as_object_mut().unwrap();
            for meta in ["updated_at", "version", "updated_by_device"] {
                object.remove(meta);
            }
            // Categories and subcategories carry no legacy id on the wire (everything
            // joins them by key), so each device mints its own.
            if table == "subcategories" || table == "categories" {
                object.remove("id");
                object.remove("_id");
            }
            value
        })
        .collect()
}

async fn keys(pool: &SqlitePool, table: &str) -> BTreeSet<String> {
    sqlx::query_scalar::<_, String>(&format!("SELECT key FROM {table}"))
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

// ---------------------------------------------------------------------------
// The scenario
// ---------------------------------------------------------------------------

#[tokio::test]
async fn three_devices_and_the_cloud_converge() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;
    let _ = Tenant::id(TENANT).unwrap();

    let mut devices = vec![
        Device::new(1, &cloud).await,
        Device::new(2, &cloud).await,
        Device::new(3, &cloud).await,
    ];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    // Device 1 is the first device: it creates the shop and uploads it.
    let catalog = create_shop(&devices[0]).await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;

    // Devices 2 and 3 join by bootstrapping from the cloud snapshot.
    for d in devices.iter_mut().skip(1) {
        bootstrap(&cloud, d).await;
    }

    // Offline work with partial, random syncs.
    let mut rng = Rng(SEED);
    let mut step = 0;
    for round in 0..ROUNDS {
        for d in &devices {
            for _ in 0..OPS_PER_ROUND {
                step += 1;
                random_op(d, &catalog, &mut rng, step).await;
            }
        }
        let syncing: Vec<usize> = (0..devices.len()).filter(|_| rng.below(3) != 0).collect();
        for i in syncing {
            push_all(&cloud, &devices[i]).await;
            pull_all(&cloud, &mut devices[i]).await;
        }
        eprintln!("round {round} done");
    }

    // Drive to quiescence: everybody pushes, the cloud settles, everybody pulls; twice.
    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // 1. Every replica holds the same data.
    for table in SYNCED {
        let base = table_rows(devices[0].pool(), table).await;
        for d in &devices[1..] {
            assert_eq!(
                base,
                table_rows(d.pool(), table).await,
                "table {table} differs on {}",
                d.id
            );
        }
    }

    let logins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(devices[0].pool())
        .await
        .unwrap();
    assert!(
        logins > 0,
        "the scenario must create logins for the users comparison to mean anything"
    );

    // 2. Stock is exactly its movement ledger on every replica.
    for d in &devices {
        let rows = sqlx::query(
            "SELECT p.key AS key, p.stock_quantity AS stock, COALESCE(SUM(m.quantity_delta), 0) AS ledger \
             FROM products p LEFT JOIN stock_movements m ON m.product_id = p.id GROUP BY p.id",
        )
        .fetch_all(d.pool())
        .await
        .unwrap();
        for row in rows {
            use sqlx::Row;
            let (key, stock, ledger): (String, i64, i64) =
                (row.get("key"), row.get("stock"), row.get("ledger"));
            assert_eq!(
                stock, ledger,
                "stock of {key} is not its ledger on {}",
                d.id
            );
        }
    }

    // 3. The cloud holds exactly the ledger the devices hold.
    let cloud_log = cloud_changes(&cloud).await;
    for (resource, table) in [
        ("stockMovements", "stock_movements"),
        ("invoices", "invoices"),
        ("payments", "payments"),
        ("users", "users"),
    ] {
        let in_cloud: BTreeSet<String> = cloud_log
            .iter()
            .filter(|c| c["resource"] == resource && c["op"] == "upsert")
            .map(|c| c["key"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            in_cloud,
            keys(devices[0].pool(), table).await,
            "{resource} in the cloud differ from the devices"
        );
    }

    // 4. No two invoices share a number, and nothing was renamed to dodge a collision.
    let numbers: Vec<String> = sqlx::query_scalar("SELECT invoice_number FROM invoices")
        .fetch_all(devices[0].pool())
        .await
        .unwrap();
    let distinct: BTreeSet<&String> = numbers.iter().collect();
    assert_eq!(
        numbers.len(),
        distinct.len(),
        "duplicate invoice numbers: {numbers:?}"
    );
    for d in &devices {
        let unique_violations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'UNIQUE_VIOLATION'",
        )
        .fetch_one(d.pool())
        .await
        .unwrap();
        assert_eq!(
            unique_violations, 0,
            "{} had to rename a colliding value",
            d.id
        );
    }

    cloud.db.drop().await.ok();
}

// ---------------------------------------------------------------------------
// Hardening: clock skew, injected conflicts, wider operation variety.
// Each test below opens its own throwaway cloud database, isolated from the
// scenario above and from each other.
// ---------------------------------------------------------------------------

/// Persistent per-device clock skew must not stop convergence: `clock_offset_ms`
/// is baked into every outbound change's `updatedAt` (see `modules::sync::state`
/// and `modules::sync::outbox`), and the receiving side clamps/monotonically
/// orders on top of that - this proves the combination actually holds across a
/// real push/pull/apply round trip, with four devices and a wider operation mix
/// than the base scenario (adds credit notes and repairs into the rotation).
#[tokio::test]
async fn devices_with_persistent_clock_skew_still_converge() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![
        Device::new(11, &cloud).await,
        Device::new(12, &cloud).await,
        Device::new(13, &cloud).await,
        Device::new(14, &cloud).await,
    ];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }
    // Device 12 runs 90s fast, device 13 runs 60s slow, the rest are on time.
    devices[1]
        .sync(
            "POST",
            "/api/sync/state",
            Some(json!({ "clockOffsetMs": 90_000 })),
        )
        .await;
    devices[2]
        .sync(
            "POST",
            "/api/sync/state",
            Some(json!({ "clockOffsetMs": -60_000 })),
        )
        .await;

    let catalog = create_shop(&devices[0]).await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    for d in devices.iter_mut().skip(1) {
        bootstrap(&cloud, d).await;
    }

    let mut rng = Rng(SEED ^ 0xC10C_5CE1);
    let mut step = 0;
    for round in 0..5 {
        for d in &devices {
            for _ in 0..4 {
                step += 1;
                random_op(d, &catalog, &mut rng, step).await;
            }
        }
        let syncing: Vec<usize> = (0..devices.len()).filter(|_| rng.below(3) != 0).collect();
        for i in syncing {
            push_all(&cloud, &devices[i]).await;
            pull_all(&cloud, &mut devices[i]).await;
        }
        eprintln!("skew round {round} done");
    }

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    for table in SYNCED {
        let base = table_rows(devices[0].pool(), table).await;
        for d in &devices[1..] {
            assert_eq!(
                base,
                table_rows(d.pool(), table).await,
                "table {table} differs on {} under clock skew",
                d.id
            );
        }
    }
    for d in &devices {
        let rows = sqlx::query(
            "SELECT p.key AS key, p.stock_quantity AS stock, COALESCE(SUM(m.quantity_delta), 0) AS ledger \
             FROM products p LEFT JOIN stock_movements m ON m.product_id = p.id GROUP BY p.id",
        )
        .fetch_all(d.pool())
        .await
        .unwrap();
        for row in rows {
            use sqlx::Row;
            let (key, stock, ledger): (String, i64, i64) =
                (row.get("key"), row.get("stock"), row.get("ledger"));
            assert_eq!(
                stock, ledger,
                "stock of {key} is not its ledger on {} under clock skew",
                d.id
            );
        }
    }
    let numbers: Vec<String> = sqlx::query_scalar("SELECT invoice_number FROM invoices")
        .fetch_all(devices[0].pool())
        .await
        .unwrap();
    let distinct: BTreeSet<&String> = numbers.iter().collect();
    assert_eq!(
        numbers.len(),
        distinct.len(),
        "duplicate invoice numbers under clock skew: {numbers:?}"
    );

    cloud.db.drop().await.ok();
}

/// Two devices independently sell the SAME physical serial (offline, unaware of
/// each other) to two different invoices. Both sales must survive the merge -
/// selling twice is a real-world mistake to flag for a human, not data to
/// silently drop - and it must be recorded as a `SERIAL_DOUBLE_SOLD` conflict.
#[tokio::test]
async fn concurrent_serial_sale_is_flagged_and_both_invoices_survive() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(21, &cloud).await, Device::new(22, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    // Device 1 mints the serialized product, its serial, and pushes them out.
    let cat = devices[0]
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Serial Goods", "icon": "phone", "color": "teal" })),
        )
        .await;
    let category_key = cat["data"]["key"].as_str().unwrap().to_string();
    let sub = devices[0]
        .api(
            "POST",
            &format!("/api/inventory/categories/{category_key}/subcategories"),
            Some(json!({ "name": "Flagship" })),
        )
        .await;
    let subcategory_key = sub["data"]["subcategories"][0]["key"]
        .as_str()
        .unwrap()
        .to_string();
    let product = devices[0]
        .api(
            "POST",
            "/api/inventory/products",
            Some(json!({
                "name": "Serial Phone", "categoryKey": category_key, "subcategoryKey": subcategory_key,
                "sellingPriceCents": 80000, "costPriceCents": 50000, "stockQuantity": 0, "minStockThreshold": 1,
                "isSerialized": true
            })),
        )
        .await;
    let product_key = product["data"]["key"].as_str().unwrap().to_string();
    let supplier = devices[0]
        .api(
            "POST",
            "/api/suppliers",
            Some(json!({
                "name": "Serial Supplier", "contactPerson": "Sam", "primaryPhone": "0112345000",
                "address": "Colombo", "suppliedCategories": ["Serial Goods"]
            })),
        )
        .await;
    let supplier_key = supplier["data"]["key"].as_str().unwrap().to_string();
    let serial = format!("SN-{}", Uuid::new_v4());
    devices[0]
        .api(
            "POST",
            "/api/purchases",
            Some(json!({
                "supplierKey": supplier_key, "productKey": product_key, "quantity": 1, "unitCostCents": 50000,
                "date": chrono::Utc::now().to_rfc3339(), "serialNumbers": [serial]
            })),
        )
        .await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    bootstrap(&cloud, &mut devices[1]).await;

    // Offline: both devices sell the SAME serial to their own new invoice.
    let mut invoice_keys = Vec::new();
    for d in &devices {
        let sale = d
            .api(
                "POST",
                "/api/billing/sales",
                Some(json!({
                    "staff": { "cashierName": "Admin" },
                    "items": [{ "productKey": product_key, "quantity": 1, "discountCents": 0, "sourceType": "retail", "serialNumbers": [serial] }],
                    "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 80000 },
                    "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
                })),
            )
            .await;
        invoice_keys.push(sale["data"]["invoice"]["key"].as_str().unwrap().to_string());
    }
    assert_ne!(invoice_keys[0], invoice_keys[1]);

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // Both sales survive, on every replica, with their own invoice number.
    for d in &devices {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invoices WHERE key IN (?, ?)")
            .bind(&invoice_keys[0])
            .bind(&invoice_keys[1])
            .fetch_one(d.pool())
            .await
            .unwrap();
        assert_eq!(
            count, 2,
            "both sales of the double-sold serial must survive on {}",
            d.id
        );
    }

    // The conflict is on record somewhere: on the cloud (it sees the second
    // push land on top of the first) and/or on whichever device pulled the
    // other's conflicting change.
    let cloud_hits = cloud_conflicts(&cloud, "SERIAL_DOUBLE_SOLD").await.len();
    let mut device_hits = 0i64;
    for d in &devices {
        device_hits += sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'SERIAL_DOUBLE_SOLD'",
        )
        .fetch_one(d.pool())
        .await
        .unwrap();
    }
    assert!(
        cloud_hits + device_hits as usize >= 1,
        "no SERIAL_DOUBLE_SOLD conflict was recorded anywhere"
    );

    // Fixed bug: `apply_mongo::serial_double_sale` used to keep whichever row was
    // *already stored*, unconditionally, discarding the incoming write even when
    // `core::sync_merge::decide` had already picked it as the winner under the
    // ordinary `(updated_at_ms, device_id)` tiebreak - so a receiving device,
    // running that same `decide` on its own pull, could reach a different answer
    // than the cloud ever actually stored. The fix makes the serial rule ADD a
    // conflict note without ever overriding which write lands, so the cloud's
    // document and every device's pulled copy are driven by the exact same
    // tiebreak and must agree. Assert that now, instead of just documenting a
    // disagreement.
    let cloud_row = cloud
        .db
        .collection::<Document>("product_serials")
        .find_one(doc! { "tenant_id": TENANT, "serial_number": &serial })
        .await
        .unwrap()
        .expect("the serial must exist on the cloud");
    let cloud_state = (
        cloud_row.get_str("status").unwrap().to_string(),
        cloud_row.get_str("invoice_key").unwrap().to_string(),
    );
    assert_eq!(cloud_state.0, "sold");
    assert!(invoice_keys.contains(&cloud_state.1));

    for d in &devices {
        let row: (String, String) = sqlx::query_as(
            "SELECT status, invoice_key FROM product_serials WHERE serial_number = ?",
        )
        .bind(&serial)
        .fetch_one(d.pool())
        .await
        .unwrap();
        assert_eq!(
            row, cloud_state,
            "{} disagrees with the cloud about who owns the serial",
            d.id
        );
    }

    cloud.db.drop().await.ok();
}

/// Two devices independently refund the SAME invoice in full, offline and
/// unaware of each other - each looks locally valid at the time. Once merged,
/// the combined refund exceeds the invoice total: this must be *flagged*, not
/// silently clamped or corrupted, and neither credit note may be dropped.
#[tokio::test]
async fn concurrent_full_refunds_raise_over_refund_not_silent_corruption() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(31, &cloud).await, Device::new(32, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
        reserve_blocks(&cloud, d).await;
    }

    let catalog = create_shop(&devices[0]).await;
    let (product_key, _product_id) = catalog.products[0].clone();
    let sale = devices[0]
        .api(
            "POST",
            "/api/billing/sales",
            Some(json!({
                "staff": { "cashierName": "Admin" },
                "items": [{ "productKey": product_key, "quantity": 2, "discountCents": 0, "sourceType": "retail" }],
                "payment": { "paymentMethod": "cash", "isCredit": false, "amountReceivedCents": 100_000_000 },
                "shopProfileSnapshot": { "name": "Shop", "address": "Colombo" }
            })),
        )
        .await;
    let invoice_key = sale["data"]["invoice"]["key"].as_str().unwrap().to_string();
    let total_cents = sale["data"]["invoice"]["totalCents"].as_i64().unwrap();

    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    bootstrap(&cloud, &mut devices[1]).await;

    // Offline: both devices independently return the full quantity.
    let mut credit_note_keys = Vec::new();
    for d in &devices {
        let cn = d
            .api(
                "POST",
                "/api/billing/credit-notes",
                Some(json!({
                    "invoiceKey": invoice_key,
                    "returnedItems": [{ "productKey": product_key, "quantity": 2, "condition": "resalable", "reason": "customer_changed_mind" }],
                    "paymentMethod": "cash"
                })),
            )
            .await;
        credit_note_keys.push(cn["data"]["key"].as_str().unwrap().to_string());
    }
    assert_ne!(credit_note_keys[0], credit_note_keys[1]);

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // Both credit notes survive, everywhere.
    for d in &devices {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM credit_notes WHERE key IN (?, ?)")
                .bind(&credit_note_keys[0])
                .bind(&credit_note_keys[1])
                .fetch_one(d.pool())
                .await
                .unwrap();
        assert_eq!(count, 2, "both credit notes must survive on {}", d.id);
    }

    // The invoice's refunded_cents is the honest (over-)sum of both notes, not
    // some clamped/negative/garbage value - and it is flagged as an OVER_REFUND.
    for d in &devices {
        let refunded: i64 = sqlx::query_scalar("SELECT refunded_cents FROM invoices WHERE key = ?")
            .bind(&invoice_key)
            .fetch_one(d.pool())
            .await
            .unwrap();
        assert!(
            refunded > total_cents,
            "{} should show the honest over-refunded total, got {refunded} for a {total_cents}-cent invoice",
            d.id
        );
        assert!(
            refunded >= 0 && refunded <= total_cents * 3,
            "{} shows a nonsensical refunded_cents: {refunded}",
            d.id
        );
    }

    let cloud_hits = cloud_conflicts(&cloud, "OVER_REFUND").await.len();
    let mut device_hits = 0i64;
    for d in &devices {
        device_hits += sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'OVER_REFUND'",
        )
        .fetch_one(d.pool())
        .await
        .unwrap();
    }
    assert!(
        cloud_hits + device_hits as usize >= 1,
        "no OVER_REFUND conflict was recorded anywhere"
    );

    cloud.db.drop().await.ok();
}

/// Two devices independently create a category with the SAME name, offline.
/// Business rules cannot stop this (each device only ever sees its own local
/// row), so the merge is the last line of defense: the losing name is renamed
/// with a device-tag suffix and reported, rather than one category silently
/// overwriting or shadowing the other.
#[tokio::test]
async fn concurrent_duplicate_category_names_are_renamed_and_both_survive() {
    let cloud = common::spawn_app_multi_tenant().await;
    start_consumer(&cloud).await;

    let mut devices = vec![Device::new(41, &cloud).await, Device::new(42, &cloud).await];
    for d in &devices {
        d.sync(
            "POST",
            "/api/sync/enable",
            Some(json!({ "tenantId": TENANT, "deviceId": d.id })),
        )
        .await;
    }
    // A shared baseline so both devices have something to bootstrap/converge on.
    devices[0]
        .api(
            "POST",
            "/api/inventory/categories",
            Some(json!({ "name": "Baseline", "icon": "box", "color": "gray" })),
        )
        .await;
    push_all(&cloud, &devices[0]).await;
    wait_for_cloud_quiet(&cloud).await;
    bootstrap(&cloud, &mut devices[1]).await;

    // Offline: both devices create a category with the identical name.
    let mut category_keys = Vec::new();
    for d in &devices {
        let cat = d
            .api(
                "POST",
                "/api/inventory/categories",
                Some(json!({ "name": "Accessories", "icon": "tag", "color": "purple" })),
            )
            .await;
        category_keys.push(cat["data"]["key"].as_str().unwrap().to_string());
    }
    assert_ne!(category_keys[0], category_keys[1]);

    for _ in 0..2 {
        for d in &devices {
            push_all(&cloud, d).await;
        }
        wait_for_cloud_quiet(&cloud).await;
        for d in devices.iter_mut() {
            pull_all(&cloud, d).await;
        }
    }

    // Both categories survive on every replica, and their names are now distinct.
    for d in &devices {
        let names: Vec<String> =
            sqlx::query_scalar("SELECT name FROM categories WHERE key IN (?, ?) ORDER BY key")
                .bind(&category_keys[0])
                .bind(&category_keys[1])
                .fetch_all(d.pool())
                .await
                .unwrap();
        assert_eq!(names.len(), 2, "both categories must survive on {}", d.id);
        assert_ne!(
            names[0], names[1],
            "the colliding name must have been renamed apart on {}",
            d.id
        );
    }
    // Fixed bug: which of the two keys kept the original name and which got the
    // device-tag suffix used to be decided independently by whichever replica
    // was CURRENTLY APPLYING when it hit the collision - device A would rename
    // B's incoming create while device B, applying A's incoming create against
    // its own already-stored row, renamed A's instead, so both devices ended up
    // with the same two names as a SET but assigned to SWAPPED keys. The fix
    // makes the winner deterministic and identical everywhere (the
    // lexicographically LOWER key always keeps the original value) and
    // republishes a locally-decided correction from inside the applying
    // transaction instead of letting it get stuck there. Assert the exact
    // per-key assignment now agrees everywhere, not just the set of names.
    let (lower_key, higher_key) = if category_keys[0] < category_keys[1] {
        (&category_keys[0], &category_keys[1])
    } else {
        (&category_keys[1], &category_keys[0])
    };

    async fn name_of(pool: &SqlitePool, key: &str) -> String {
        sqlx::query_scalar("SELECT name FROM categories WHERE key = ?")
            .bind(key)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    async fn cloud_name_of(cloud: &common::TestApp, key: &str) -> String {
        cloud
            .db
            .collection::<Document>("categories")
            .find_one(doc! { "tenant_id": TENANT, "key": key })
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("{key} must exist on the cloud"))
            .get_str("name")
            .unwrap()
            .to_string()
    }
    let cloud_lower = cloud_name_of(&cloud, lower_key).await;
    let cloud_higher = cloud_name_of(&cloud, higher_key).await;
    assert_eq!(
        cloud_lower, "Accessories",
        "the lower key must keep the original name"
    );
    assert_ne!(
        cloud_higher, "Accessories",
        "the higher key must have been renamed apart"
    );

    for d in &devices {
        assert_eq!(
            name_of(d.pool(), lower_key).await,
            cloud_lower,
            "{} disagrees with the cloud on the lower key's name",
            d.id
        );
        assert_eq!(
            name_of(d.pool(), higher_key).await,
            cloud_higher,
            "{} disagrees with the cloud on the higher key's name",
            d.id
        );
    }

    let cloud_hits = cloud_conflicts(&cloud, "UNIQUE_VIOLATION").await.len();
    let mut device_hits = 0i64;
    for d in &devices {
        device_hits += sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sync_conflicts WHERE kind = 'UNIQUE_VIOLATION'",
        )
        .fetch_one(d.pool())
        .await
        .unwrap();
    }
    assert!(
        cloud_hits + device_hits as usize >= 1,
        "no UNIQUE_VIOLATION conflict was recorded anywhere"
    );

    cloud.db.drop().await.ok();
}
