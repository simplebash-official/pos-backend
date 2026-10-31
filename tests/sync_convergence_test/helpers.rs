//! Shared setup for the `sync_convergence_test` tests.

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{Request, StatusCode};
use jsonwebtoken::{EncodingKey, Header, encode};
use mongodb::bson::{Document, doc};
use serde_json::{Value, json};
use simplebash_pos_backend::core::constants::roles;

use crate::common;
use simplebash_pos_backend::domain::users::Role;
use simplebash_pos_backend::modules::sync::cloud_capture::run_consumer;
use sqlx::SqlitePool;
use std::collections::BTreeSet;
use tower::ServiceExt;
use uuid::Uuid;

// End-to-end convergence: three offline-capable SQLite devices (the real desktop
// backend router each) and one multi-tenant cloud (real router, real MongoDB
// Atlas in a throwaway database, real change-stream consumer) talk through the
// same HTTP contracts the desktop sync agent uses. A seeded scenario makes each
// device work offline (sales, price edits, stock edits, new customers), syncs a
// random subset per round, then everything is driven to quiescence. Afterwards
// every replica must hold the same data, stock must equal its movement ledger
// on every device, and the cloud must hold exactly the ledger the devices do.

pub(crate) const TENANT: &str = "tnt_convergence";

pub(crate) const SEED: u64 = 0x5EED_C0DE;

// ---------------------------------------------------------------------------
// Tiny deterministic RNG (the scenario must be reproducible from SEED alone)
// ---------------------------------------------------------------------------

pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

pub(crate) async fn call(
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

// ---------------------------------------------------------------------------
// A device: the real desktop backend on in-memory SQLite
// ---------------------------------------------------------------------------

pub(crate) struct Device {
    pub(crate) app: common::TestApp,
    pub(crate) admin: String,
    pub(crate) agent: String,
    pub(crate) id: String,
    pub(crate) cloud_token: String,
    pub(crate) cursor: i64,
}

impl Device {
    pub(crate) async fn new(n: usize, cloud: &common::TestApp) -> Device {
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
        .expect("the token encodes");
        let id = format!("dev_conv{n}aaaa{n}bbbb");
        let cloud_token = common::registered_device_token(cloud, TENANT, &id).await;
        Device {
            app,
            admin,
            agent,
            id,
            cloud_token,
            cursor: 0,
        }
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        self.app.db_handle.as_sqlite().expect("sqlite device")
    }

    pub(crate) async fn api(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let (status, value) = call(&self.app.router, method, uri, &self.admin, body).await;
        assert!(
            status.is_success(),
            "[{}] {method} {uri} -> {status}: {value}",
            self.id
        );
        value
    }

    pub(crate) async fn sync(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
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
pub(crate) async fn push_all(cloud: &common::TestApp, d: &Device) {
    loop {
        let page = d
            .sync("GET", "/api/sync/outbox?after=0&limit=200", None)
            .await;
        let items = page["items"]
            .as_array()
            .expect("the value is an array")
            .clone();
        if items.is_empty() {
            return;
        }
        let changes: Vec<Value> = items.iter().map(|i| i["record"].clone()).collect();
        let seqs: Vec<i64> = items
            .iter()
            .map(|i| i["seq"].as_i64().expect("the value is an integer"))
            .collect();
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
        for ack in body["data"]["acks"]
            .as_array()
            .expect("the value is an array")
        {
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
pub(crate) async fn pull_all(cloud: &common::TestApp, d: &mut Device) {
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
        let next = data["nextSeq"].as_i64().expect("the value is an integer");
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
pub(crate) async fn bootstrap(cloud: &common::TestApp, d: &mut Device) {
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
                .expect("the value is an array")
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
                as_of = data["asOfSeq"].as_i64().expect("the value is an integer");
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

/// Reserve a block of invoice and quotation numbers from the cloud and hand them to the device.
pub(crate) async fn reserve_blocks(cloud: &common::TestApp, d: &Device) {
    for name in ["invoice", "quotation"] {
        let (status, body) = call(
            &cloud.router,
            "POST",
            &format!("/api/sequences/{name}/reserve"),
            &d.cloud_token,
            Some(json!({ "blockSize": 200, "deviceId": d.id })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "[{}] reserve {name}: {body}", d.id);
        let b = &body["data"];
        d.sync(
            "POST",
            "/api/sync/blocks",
            Some(json!({
                "name": name, "prefix": b["prefix"], "padding": b["padding"],
                "start": b["start"], "end": b["end"], "expiresAt": b["expiresAt"]
            })),
        )
        .await;
    }
}

/// Every change the cloud holds, read as a device that never wrote anything.
pub(crate) async fn cloud_changes(cloud: &common::TestApp) -> Vec<Value> {
    let observer = common::registered_device_token(cloud, TENANT, "dev_observer").await;
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
        since = data["nextSeq"].as_i64().expect("the value is an integer");
        if !data["hasMore"].as_bool().unwrap_or(false) {
            return all;
        }
    }
}

/// Conflicts of a given kind the cloud has recorded for `TENANT` (`cloud_capture`
/// and `push` write to `sync_conflicts` the same way the device applier does).
pub(crate) async fn cloud_conflicts(cloud: &common::TestApp, kind: &str) -> Vec<Document> {
    use futures_util::TryStreamExt;
    cloud
        .mongo()
        .collection::<Document>("sync_conflicts")
        .find(doc! { "tenant_id": TENANT, "kind": kind })
        .await
        .expect("the awaited step succeeds")
        .try_collect()
        .await
        .expect("the awaited step succeeds")
}

/// The change-stream consumer is asynchronous: wait until the cloud's log stops growing.
pub(crate) async fn wait_for_cloud_quiet(cloud: &common::TestApp) {
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
pub(crate) async fn start_consumer(cloud: &common::TestApp) {
    cloud
        .mongo()
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
    let observer = common::registered_device_token(cloud, "tnt_canary", "dev_canary").await;
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

pub(crate) async fn table_rows(pool: &SqlitePool, table: &str) -> Vec<Value> {
    use simplebash_pos_backend::clients::sqlite::map_sqlite_row_to_document;
    let rows = sqlx::query(&format!("SELECT * FROM {table} ORDER BY key"))
        .fetch_all(pool)
        .await
        .expect("the awaited step succeeds");
    rows.iter()
        .map(|row| {
            let mut value = serde_json::to_value(map_sqlite_row_to_document(row))
                .expect("the row converts to JSON");
            let object = value.as_object_mut().expect("the value is an object");
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

pub(crate) async fn keys(pool: &SqlitePool, table: &str) -> BTreeSet<String> {
    sqlx::query_scalar::<_, String>(&format!("SELECT key FROM {table}"))
        .fetch_all(pool)
        .await
        .expect("the awaited step succeeds")
        .into_iter()
        .collect()
}
