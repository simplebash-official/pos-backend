// Realtime "something changed" notifications for sync, so replicas react in
// milliseconds instead of waiting for their next poll. Notifications carry no
// record data (only a sequence number, the resource name and who wrote it):
// receivers still read the change through the normal pull or REST endpoints,
// which keeps permissions and payload shapes exactly where they are.
//
// Two independent channels live here:
//
// - Cloud (`TENANT_MODE=multi`): `run_watcher` watches inserts into the
//   tenant-scoped `sync_changes` log and fans each one out to that tenant's
//   subscribers of `GET /api/sync/events` (devices and browser tabs). Every
//   instance runs its own watcher, so subscribers can be served by any
//   instance without a lease.
// - Desktop (SQLite): `local_write_tick` marks every successful write request,
//   and `GET /api/sync/local/events` turns those marks into a stream for the
//   desktop shell's sync agent, which then uploads at once.
//
// Both use process-wide registries rather than `AppState` fields: they are
// keyed by tenant (cloud) or are per-process by nature (desktop).

use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use axum::{
    extract::Request,
    http::Method,
    middleware::Next,
    response::{
        Response,
        sse::{Event, KeepAlive},
    },
};
use futures_util::{Stream, StreamExt, stream};
use mongodb::bson::{Document, doc};
use serde::Serialize;
use serde_json::json;
use tokio::sync::{broadcast, watch};
use utoipa::ToSchema;

use crate::{
    clients::db::Db,
    core::{
        error::{AppError, AppResult},
        tenancy::TENANT_FIELD,
    },
    modules::sync::cloud_store::CHANGES,
};

/// Comment line sent while idle, so proxies keep the stream open and clients
/// notice a dead connection.
pub const HEARTBEAT: Duration = Duration::from_secs(15);
/// Notifications buffered per tenant before a slow subscriber is told to
/// resync (it then pulls, which loses nothing).
const CHANNEL_CAPACITY: usize = 1024;
/// Longest single wait for new change-log rows; the server answers at once
/// when one arrives.
const MAX_AWAIT: Duration = Duration::from_millis(500);
const WATCH_RETRY: Duration = Duration::from_secs(1);

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

// ----------------------------------------------------------------------------
// Cloud: per-tenant fan-out
// ----------------------------------------------------------------------------

fn tenants() -> &'static Mutex<HashMap<String, broadcast::Sender<LiveEvent>>> {
    static TENANTS: OnceLock<Mutex<HashMap<String, broadcast::Sender<LiveEvent>>>> =
        OnceLock::new();
    TENANTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Receives every change of `tenant` announced from now on.
pub fn subscribe(tenant: &str) -> broadcast::Receiver<LiveEvent> {
    let mut map = tenants().lock().unwrap_or_else(|e| e.into_inner());
    map.entry(tenant.to_string())
        .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
        .subscribe()
}

/// Announces a change to the tenant's subscribers (dropping the channel once
/// nobody listens any more).
pub fn publish(tenant: &str, event: LiveEvent) {
    let mut map = tenants().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(sender) = map.get(tenant)
        && sender.send(event).is_err()
    {
        map.remove(tenant);
    }
}

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

fn live_event_of(row: &Document) -> Option<(String, LiveEvent)> {
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

/// SSE keep-alive used by both channels.
pub fn keep_alive() -> KeepAlive {
    KeepAlive::new().interval(HEARTBEAT).text("ping")
}

// ----------------------------------------------------------------------------
// Desktop: "the outbox may have grown"
// ----------------------------------------------------------------------------

fn local_writes() -> &'static watch::Sender<u64> {
    static WRITES: OnceLock<watch::Sender<u64>> = OnceLock::new();
    WRITES.get_or_init(|| watch::channel(0).0)
}

/// Sync-agent calls that never add anything to upload (or are the upload
/// itself); everything else that writes may have queued a change.
const NOT_LOCAL_CHANGES: &[&str] = &[
    "/api/sync/apply",
    "/api/sync/outbox/ack",
    "/api/sync/state",
    "/api/sync/blocks",
    "/api/logs",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribers_get_other_writers_changes_and_never_their_own() {
        let receiver = subscribe("t_live_1");
        let events = cloud_events(receiver, 7, "dev_a".to_string());
        tokio::pin!(events);
        // hello first
        assert!(events.next().await.is_some());

        publish(
            "t_live_1",
            LiveEvent {
                seq: 8,
                resource: "products".into(),
                origin: Some("dev_a".into()),
            },
        );
        publish(
            "t_live_1",
            LiveEvent {
                seq: 9,
                resource: "customers".into(),
                origin: Some("web:x".into()),
            },
        );
        let next = tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("an event")
            .expect("open");
        assert!(next.is_ok());
        // Another tenant's change is never seen.
        publish(
            "t_live_other",
            LiveEvent {
                seq: 1,
                resource: "products".into(),
                origin: None,
            },
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), events.next())
                .await
                .is_err()
        );
    }

    #[test]
    fn change_log_rows_become_events() {
        let row = doc! {
            "tenant_id": "t1", "seq": 12_i64, "resource": "invoices", "origin_device_id": "dev_9",
        };
        let (tenant, event) = live_event_of(&row).unwrap();
        assert_eq!(tenant, "t1");
        assert_eq!(
            event,
            LiveEvent {
                seq: 12,
                resource: "invoices".into(),
                origin: Some("dev_9".into())
            }
        );
    }

    #[tokio::test]
    async fn local_stream_announces_at_once_then_after_each_write() {
        let events = local_events();
        tokio::pin!(events);
        assert!(events.next().await.is_some(), "initial outbox event");
        local_writes().send_modify(|n| *n += 1);
        assert!(
            tokio::time::timeout(Duration::from_secs(1), events.next())
                .await
                .is_ok()
        );
    }
}
