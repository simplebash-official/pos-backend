use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::tenancy::TENANT_FIELD;
use crate::modules::sync::cloud_store::CHANGES;
use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use axum::response::sse::{Event, KeepAlive};
use futures_util::{Stream, StreamExt, stream};
use mongodb::bson::{Document, doc};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use utoipa::ToSchema;

/// The SSE body of `GET /api/sync/events`: `hello` with the latest position,
/// then one `change` per announced row (the caller's own writes left out),
/// or `resync` when this subscriber fell too far behind to be told each one.
/// `receiver` must be subscribed before `latest_seq` is read, so nothing
/// written in between is missed.
pub fn cloud_events(
    receiver: broadcast::Receiver<LiveEvent>,
    latest_seq: i64,
    caller_origin: String,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let hello = Event::default()
        .event("hello")
        .data(json!({ "latestSeq": latest_seq }).to_string());
    let changes = stream::unfold(
        (receiver, caller_origin),
        |(mut receiver, caller_origin)| async move {
            loop {
                let event = match receiver.recv().await {
                    Ok(event) if event.origin.as_deref() == Some(caller_origin.as_str()) => {
                        continue;
                    }
                    Ok(event) => Event::default()
                        .event("change")
                        .id(event.seq.to_string())
                        .data(serde_json::to_string(&event).unwrap_or_default()),
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        Event::default().event("resync").data("{}")
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                };
                return Some((Ok(event), (receiver, caller_origin)));
            }
        },
    );
    stream::once(async move { Ok(hello) }).chain(changes)
}

/// Comment line sent while idle, so proxies keep the stream open and clients
/// notice a dead connection.
pub const HEARTBEAT: Duration = Duration::from_secs(15);

/// SSE keep-alive used by both channels.
pub fn keep_alive() -> KeepAlive {
    KeepAlive::new().interval(HEARTBEAT).text("ping")
}

/// One appended change-log row, as announced to subscribers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LiveEvent {
    /// Position in the tenant's change log; pull with `since` below it.
    pub seq: i64,
    /// Wire resource name, e.g. `products`.
    pub resource: String,
    /// Who made the change: a device id, `web:<browser id>` or `cloud`.
    pub origin: Option<String>,
}

pub(super) fn live_event_of(row: &Document) -> Option<(String, LiveEvent)> {
    let tenant = row.get_str(TENANT_FIELD).ok()?.to_string();
    let seq = match row.get("seq")? {
        mongodb::bson::Bson::Int64(n) => *n,
        mongodb::bson::Bson::Int32(n) => i64::from(*n),
        _ => return None,
    };
    Some((
        tenant,
        LiveEvent {
            seq,
            resource: row.get_str("resource").ok()?.to_string(),
            origin: row.get_str("origin_device_id").ok().map(str::to_string),
        },
    ))
}

/// The SSE body of `GET /api/sync/local/events`: one `outbox` event at once
/// (anything already waiting) and one after each local write, coalesced
/// while the agent is busy.
pub fn local_events() -> impl Stream<Item = Result<Event, Infallible>> {
    let mut receiver = local_writes().subscribe();
    receiver.mark_changed();
    stream::unfold(receiver, |mut receiver| async move {
        receiver.changed().await.ok()?;
        let tick = *receiver.borrow_and_update();
        Some((
            Ok(Event::default().event("outbox").data(tick.to_string())),
            receiver,
        ))
    })
}

/// Sync-agent calls that never add anything to upload (or are the upload
/// itself); everything else that writes may have queued a change.
const NOT_LOCAL_CHANGES: &[&str] = &[
    "/api/sync/apply",
    "/api/sync/outbox/ack",
    "/api/sync/state",
    "/api/sync/blocks",
    "/api/logs",
    // Live calculations store nothing.
    "/api/calc",
];

/// Middleware: after every successful write request, tell the local event
/// stream. Writes that queued nothing cost the agent one empty outbox read.
pub async fn local_write_tick(request: Request, next: Next) -> Response {
    let writes = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) && !NOT_LOCAL_CHANGES
        .iter()
        .any(|p| request.uri().path().starts_with(p));
    let response = next.run(request).await;
    if writes && response.status().is_success() {
        local_writes().send_modify(|n| *n = n.wrapping_add(1));
    }
    response
}

// ----------------------------------------------------------------------------
// Desktop: "the outbox may have grown"
// ----------------------------------------------------------------------------
pub(super) fn local_writes() -> &'static watch::Sender<u64> {
    static WRITES: OnceLock<watch::Sender<u64>> = OnceLock::new();
    WRITES.get_or_init(|| watch::channel(0).0)
}

/// Announces a change to the tenant's subscribers (dropping the channel once
/// nobody listens any more).
pub fn publish(tenant: &str, event: LiveEvent) {
    let mut map = tenants()
        .lock()
        .expect("live sync channel registry mutex poisoned");
    if let Some(sender) = map.get(tenant)
        && sender.send(event).is_err()
    {
        map.remove(tenant);
    }
}

/// Longest single wait for new change-log rows; the server answers at once
/// when one arrives.
const MAX_AWAIT: Duration = Duration::from_millis(500);

const WATCH_RETRY: Duration = Duration::from_secs(1);

/// Runs the change-log watcher forever. Spawn once per process when
/// multi-tenant.
pub async fn run_watcher(db: Db) -> ! {
    loop {
        if let Err(err) = watch_change_log(&db).await {
            tracing::warn!(%err, "sync live watcher stopped; restarting");
        }
        tokio::time::sleep(WATCH_RETRY).await;
    }
}

async fn watch_change_log(db: &Db) -> AppResult<()> {
    let log = db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .unscoped()
        .collection::<Document>(CHANGES);
    let pipeline = vec![
        doc! { "$match": { "operationType": "insert" } },
        // `operationType` must survive the projection: the driver needs it
        // to decode each event.
        doc! { "$project": {
            "operationType": 1,
            "fullDocument.tenant_id": 1,
            "fullDocument.seq": 1,
            "fullDocument.resource": 1,
            "fullDocument.origin_device_id": 1,
        }},
    ];
    // Starts at "now": a subscriber that missed earlier rows learns the
    // latest position from its `hello` event and pulls.
    let mut changes = log
        .watch()
        .pipeline(pipeline)
        .max_await_time(MAX_AWAIT)
        .await?;
    // One awaited round trip per turn (`next_if_any`), never `next()`: on a
    // stream the server has closed (collection dropped, cursor killed) `next()`
    // spins without yielding and would starve the whole runtime. A closed
    // stream ends this call and `run_watcher` reopens it.
    while changes.is_alive() {
        let Some(event) = changes.next_if_any().await? else {
            continue;
        };
        let Some(row) = event.full_document else {
            continue;
        };
        if let Some((tenant, live)) = live_event_of(&row) {
            publish(&tenant, live);
        }
    }
    Ok(())
}

/// Notifications buffered per tenant before a slow subscriber is told to
/// resync (it then pulls, which loses nothing).
const CHANNEL_CAPACITY: usize = 1024;

/// Receives every change of `tenant` announced from now on.
#[must_use]
pub fn subscribe(tenant: &str) -> broadcast::Receiver<LiveEvent> {
    let mut map = tenants()
        .lock()
        .expect("live sync channel registry mutex poisoned");
    map.entry(tenant.to_string())
        .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
        .subscribe()
}

// ----------------------------------------------------------------------------
// Cloud: per-tenant fan-out
// ----------------------------------------------------------------------------
pub(super) fn tenants() -> &'static Mutex<HashMap<String, broadcast::Sender<LiveEvent>>> {
    static TENANTS: OnceLock<Mutex<HashMap<String, broadcast::Sender<LiveEvent>>>> =
        OnceLock::new();
    TENANTS.get_or_init(|| Mutex::new(HashMap::new()))
}
