// Change capture for the cloud: ONE consumer per deployment watches the whole
// database with a MongoDB change stream and appends every write to a syncable
// collection to the writing tenant's `sync_changes` log, assigning the
// per-tenant `seq` that devices pull by. Because capture sits below the
// application, it sees every write path alike - REST handlers, pushed device
// changes, atomic pipeline updates - with nothing in the service layer having
// to remember to log.
//
// Exactly one instance consumes at a time (a lease document in the platform
// `sync_consumer` collection, renewed while it runs); standby instances keep
// serving push/pull. The stream position (resume token) is persisted, so a
// restart continues where it stopped; re-delivered events are dropped by a
// per-event `source_token`.
//
// If the saved position is no longer usable (the server dropped that part of its
// history, e.g. after the deployment was down for longer than the oplog window:
// `ChangeStreamHistoryLost`), retrying it can never succeed. The consumer then
// opens a fresh stream and re-publishes every document changed since it last
// saved its position (`catch_up`), so nothing is lost and it does not spin on the
// same error forever.
//
// A hard delete leaves the stream nothing to read (no document, no tenant), so
// the repositories that delete outright also insert a marker into
// `sync_tombstones` (`cloud_store::record_hard_deletes`), published here as a
// delete. Subcategory writes are not watched: they bump their parent
// category, whose own event carries them.

use std::{collections::HashMap, time::Duration};

use futures_util::TryStreamExt;

use chrono::Utc;
use mongodb::{
    bson::{self, Bson, DateTime as BsonDateTime, Document, doc},
    change_stream::event::{ChangeStreamEvent, ResumeToken},
    options::FullDocumentType,
};

use crate::{
    clients::db::Db,
    core::{
        error::{AppError, AppResult},
        id::generate_id,
        tenancy::{DENY_TENANT, Tenant, with_tenant},
    },
    modules::sync::{
        cloud_store::{self, CHANGES, TOMBSTONES, coll},
        compaction,
        resources::{Phase, ordered},
        service::hydrate,
    },
};

const LEASE_COLLECTION: &str = "sync_consumer";
const LEASE_ID: &str = "consumer";
const LEASE_SECS: i64 = 30;
const RENEW_EVERY: Duration = Duration::from_secs(10);
const PERSIST_EVERY: Duration = Duration::from_secs(5);
/// Longest a single wait for new events lasts. The server answers as soon as
/// an event arrives, so this only bounds how late lease renewal can run.
const MAX_AWAIT: Duration = Duration::from_millis(500);
const COMPACT_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// Attempts per event before the consumer restarts from its last saved
/// position (replaying, never skipping, the event).
const EVENT_ATTEMPTS: u32 = 5;
/// Restarts over the same event before it is given up as unpublishable.
const POISON_RESTARTS: u32 = 3;
/// Server error codes meaning "this resume token can never work": history lost (286), fatal
/// stream error (280), invalid token (260) and a token the server cannot even decode (50811).
const UNUSABLE_POSITION_CODES: [i32; 4] = [286, 280, 260, 50811];
/// Catch-up starts this long before the last saved position, so a change written just before
/// the position was saved is not missed (duplicates are harmless: applying a change twice is a no-op).
const CATCH_UP_MARGIN_MS: i64 = 5 * 60 * 1000;
/// Wait before re-taking the lease after a stop.
const RESTART_DELAY: Duration = Duration::from_secs(1);
const STANDBY_DELAY: Duration = Duration::from_secs(5);

/// Why the consumer stopped consuming. The errors are boxed so this stays a
/// small `Err` type (`AppError` alone is over 100 bytes).
enum Stop {
    /// Lease lost, stream ended or a database error: retake and resume.
    Retry(Box<AppError>),
    /// This event could not be published even after retries.
    Event {
        source_token: String,
        err: Box<AppError>,
    },
}

impl From<AppError> for Stop {
    fn from(err: AppError) -> Self {
        Stop::Retry(Box::new(err))
    }
}

impl From<mongodb::error::Error> for Stop {
    fn from(err: mongodb::error::Error) -> Self {
        Stop::Retry(Box::new(err.into()))
    }
}

/// Runs the consumer forever. Spawn once per process when multi-tenant.
pub async fn run_consumer(db: Db) -> ! {
    let holder = generate_id("cns");
    // Compaction is slow and independent of capture order, so it never runs
    // inside the capture loop (where it would hold every device's feed).
    tokio::spawn(compaction_loop(db.clone()));
    // Events that failed every attempt, by how many restarts they caused.
    let mut failing: HashMap<String, u32> = HashMap::new();
    loop {
        let mut delay = STANDBY_DELAY;
        match try_acquire_lease(&db, &holder).await {
            Ok(true) => {
                tracing::info!(%holder, "sync change consumer acquired the lease");
                let skip: Vec<String> = failing
                    .iter()
                    .filter(|(_, n)| **n >= POISON_RESTARTS)
                    .map(|(t, _)| t.clone())
                    .collect();
                match consume(&db, &holder, &skip).await {
                    Ok(()) => {}
                    Err(Stop::Retry(err)) => {
                        tracing::warn!(%err, "sync change consumer stopped; will retry");
                    }
                    Err(Stop::Event { source_token, err }) => {
                        let n = failing.entry(source_token).or_insert(0);
                        *n += 1;
                        tracing::error!(%err, restarts = *n, "a change could not be captured; replaying from the last saved position");
                    }
                }
                delay = RESTART_DELAY;
            }
            Ok(false) => {}
            Err(err) => tracing::warn!(%err, "sync consumer lease check failed"),
        }
        tokio::time::sleep(delay).await;
    }
}

async fn compaction_loop(db: Db) {
    loop {
        tokio::time::sleep(COMPACT_EVERY).await;
        match compaction::compact_all(&db, compaction::RETENTION_DAYS).await {
            Ok(n) => tracing::info!(dropped = n, "sync change log compacted"),
            Err(err) => tracing::warn!(%err, "sync change log compaction failed"),
        }
    }
}

fn lease_collection(db: &Db) -> AppResult<crate::clients::tenant_db::ScopedCollection<Document>> {
    Ok(db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .platform_collection::<Document>(LEASE_COLLECTION))
}

fn lease_until() -> BsonDateTime {
    BsonDateTime::from_millis(Utc::now().timestamp_millis() + LEASE_SECS * 1000)
}

/// Takes the lease if it is free, expired or already ours.
async fn try_acquire_lease(db: &Db, holder: &str) -> AppResult<bool> {
    let now = BsonDateTime::now();
    let result = lease_collection(db)?
        .find_one_and_update(
            doc! { "_id": LEASE_ID, "$or": [
                { "lease_until": { "$lt": now } },
                { "holder": holder },
            ]},
            doc! { "$set": { "holder": holder, "lease_until": lease_until() } },
        )
        .upsert(true)
        .await;
    match result {
        Ok(_) => Ok(true),
        // The lease document exists and is held: the upsert collides on `_id`.
        Err(err) if err.to_string().contains("E11000") => Ok(false),
        Err(err) => Err(err.into()),
    }
}

/// Extends our lease; `false` means it was lost to another instance.
async fn renew_lease(db: &Db, holder: &str) -> AppResult<bool> {
    let updated = lease_collection(db)?
        .update_one(
            doc! { "_id": LEASE_ID, "holder": holder },
            doc! { "$set": { "lease_until": lease_until() } },
        )
        .await?;
    Ok(updated.matched_count > 0)
}

async fn load_resume_token(db: &Db) -> AppResult<Option<ResumeToken>> {
    let stored = lease_collection(db)?
        .find_one(doc! { "_id": LEASE_ID })
        .await?
        .and_then(|d| d.get("resume_token").cloned());
    Ok(stored.and_then(|b| bson::deserialize_from_bson::<ResumeToken>(b).ok()))
}

async fn persist_resume_token(db: &Db, holder: &str, token: &ResumeToken) -> AppResult<()> {
    let value = bson::serialize_to_bson(token)
        .map_err(|e| AppError::internal(format!("cannot store resume token: {e}")))?;
    lease_collection(db)?
        .update_one(
            doc! { "_id": LEASE_ID, "holder": holder },
            // The time lets `catch_up` know how far back to look if this position is ever lost.
            doc! { "$set": { "resume_token": value, "resume_token_at": BsonDateTime::now() } },
        )
        .await?;
    Ok(())
}

/// True when the server says the saved position can never be resumed from.
fn position_unusable(err: &mongodb::error::Error) -> bool {
    use mongodb::error::ErrorKind;
    match err.kind.as_ref() {
        ErrorKind::Command(c) => UNUSABLE_POSITION_CODES.contains(&c.code),
        _ => false,
    }
}

/// When the saved position was last written, if ever.
async fn load_resume_token_at(db: &Db) -> AppResult<Option<BsonDateTime>> {
    Ok(lease_collection(db)?
        .find_one(doc! { "_id": LEASE_ID })
        .await?
        .and_then(|d| d.get_datetime("resume_token_at").ok().copied()))
}

/// Forgets the unusable position, so the next stream starts from now.
async fn clear_resume_token(db: &Db, holder: &str) -> AppResult<()> {
    lease_collection(db)?
        .update_one(
            doc! { "_id": LEASE_ID, "holder": holder },
            doc! { "$unset": { "resume_token": "" }, "$set": { "resume_token_at": BsonDateTime::now() } },
        )
        .await?;
    Ok(())
}

/// Re-publishes everything written since `since` (the last saved position, minus a margin): every
/// document of a syncable collection changed after it, and every hard-delete marker. Each row gets
/// its own `source_token`, so a document published twice just produces a second, identical change.
async fn catch_up(db: &Db, since: BsonDateTime) -> AppResult<usize> {
    let raw = db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .unscoped()
        .clone();
    let since = BsonDateTime::from_millis(since.timestamp_millis() - CATCH_UP_MARGIN_MS);
    let mut published = 0usize;

    for spec in ordered().filter(|s| s.phase == Phase::P3a) {
        let mut cursor = raw
            .collection::<Document>(spec.table)
            .find(doc! { "updated_at": { "$gte": since } })
            .await?;
        while let Some(full) = cursor.try_next().await? {
            let Ok(tenant_id) = full.get_str("tenant_id") else {
                continue;
            };
            if tenant_id == DENY_TENANT {
                continue;
            }
            let tenant = Tenant::id(tenant_id)?;
            let key = full.get_str("key").unwrap_or_default().to_string();
            let version = crate::modules::sync::apply_mongo::num_i64(&full, "version").unwrap_or(1);
            let updated_ms = full
                .get_datetime("updated_at")
                .map(|t| t.timestamp_millis())
                .unwrap_or_default();
            let token = format!("catchup:{}:{key}:{updated_ms}:{version}", spec.table);
            with_tenant(tenant, publish(db, spec.table, full, &token)).await?;
            published += 1;
        }
    }

    let mut markers = raw
        .collection::<Document>(TOMBSTONES)
        .find(doc! { "deleted_at": { "$gte": since } })
        .await?;
    while let Some(marker) = markers.try_next().await? {
        let Ok(tenant_id) = marker.get_str("tenant_id") else {
            continue;
        };
        if tenant_id == DENY_TENANT {
            continue;
        }
        let tenant = Tenant::id(tenant_id)?;
        let token = format!(
            "catchup:tombstone:{}",
            marker
                .get("_id")
                .map(|id| id.to_string())
                .unwrap_or_default()
        );
        with_tenant(tenant, publish_tombstone(db, marker, &token)).await?;
        published += 1;
    }
    Ok(published)
}

/// Watches the database until the lease is lost or the stream fails. Events
/// whose token is in `skip` already failed across several restarts and are
/// dropped (logged) so one unpublishable write cannot stop every tenant's feed.
async fn consume(db: &Db, holder: &str, skip: &[String]) -> Result<(), Stop> {
    let raw = db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .unscoped()
        .clone();

    let mut tables: Vec<&str> = ordered()
        .filter(|s| s.phase == Phase::P3a)
        .map(|s| s.table)
        .collect();
    tables.push(TOMBSTONES);

    let pipeline = vec![doc! { "$match": {
        "ns.coll": { "$in": tables },
        "operationType": { "$in": ["insert", "update", "replace"] },
    }}];
    let open = |token: Option<ResumeToken>| {
        let mut watch = raw
            .watch()
            .pipeline(pipeline.clone())
            .full_document(FullDocumentType::UpdateLookup)
            .max_await_time(MAX_AWAIT);
        if let Some(token) = token {
            watch = watch.start_after(token);
        }
        watch
    };
    let saved = load_resume_token(db).await?;
    let mut stream = match open(saved.clone()).await {
        Ok(stream) => stream,
        Err(err) if saved.is_some() && position_unusable(&err) => {
            // Retrying this position can never work. Start from now FIRST (so nothing written
            // from here on is missed), then re-publish what was written while we were away.
            let since = load_resume_token_at(db).await?;
            tracing::error!(%err, "the saved sync position was lost on the server; starting a fresh stream and catching up");
            clear_resume_token(db, holder).await?;
            let stream = open(None).await?;
            match since {
                Some(since) => {
                    let published = catch_up(db, since).await?;
                    tracing::warn!(
                        published,
                        "sync catch-up finished after losing the saved position"
                    );
                }
                None => tracing::error!(
                    "no time was recorded for the lost sync position, so changes made while the consumer was away cannot be replayed; devices should re-download the shop"
                ),
            }
            stream
        }
        Err(err) => return Err(err.into()),
    };

    let mut since_persist = 0u32;
    let mut last_persist = std::time::Instant::now();
    let mut last_renew = std::time::Instant::now();

    loop {
        // A stream the server closed (database dropped, cursor killed) would
        // answer at once with nothing, forever: stop and let `run_consumer`
        // reopen it from the saved position.
        if !stream.is_alive() {
            return Err(AppError::internal("the change stream was closed").into());
        }
        // One awaited round trip: returns as soon as an event exists, or
        // empty after `MAX_AWAIT`, so there is no idle sleep adding latency.
        if let Some(event) = stream.next_if_any().await? {
            let token = event_token(&event);
            if token.as_ref().is_some_and(|t| skip.contains(t)) {
                tracing::error!(source_token = ?token, "skipping a change that repeatedly failed to capture");
            } else {
                process_with_retry(db, event, token).await?;
            }
            since_persist += 1;
        }

        if last_renew.elapsed() >= RENEW_EVERY {
            if !renew_lease(db, holder).await? {
                tracing::warn!("sync consumer lost its lease");
                return Ok(());
            }
            last_renew = std::time::Instant::now();
        }
        if since_persist >= 100 || (since_persist > 0 && last_persist.elapsed() >= PERSIST_EVERY) {
            if let Some(token) = stream.resume_token() {
                persist_resume_token(db, holder, &token).await?;
            }
            since_persist = 0;
            last_persist = std::time::Instant::now();
        }
    }
}

/// The event id, which is unique per event: the natural de-duplication key.
fn event_token(event: &ChangeStreamEvent<Document>) -> Option<String> {
    bson::serialize_to_bson(&event.id).ok().and_then(|b| {
        b.as_document()
            .and_then(|d| d.get_str("_data").ok().map(str::to_string))
    })
}

/// Publishes one event, retrying transient failures. An event that still
/// fails stops the consumer without saving its position, so the event is
/// replayed after the restart instead of silently missing from the feed.
async fn process_with_retry(
    db: &Db,
    event: ChangeStreamEvent<Document>,
    token: Option<String>,
) -> Result<(), Stop> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match process_event(db, &event).await {
            Ok(()) => return Ok(()),
            Err(err) if attempt < EVENT_ATTEMPTS => {
                tracing::warn!(%err, attempt, "capturing a change failed; retrying");
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
            }
            Err(err) => {
                return Err(Stop::Event {
                    source_token: token.unwrap_or_default(),
                    err: Box::new(err),
                });
            }
        }
    }
}

/// Publishes one database event to its tenant's change log.
async fn process_event(db: &Db, event: &ChangeStreamEvent<Document>) -> AppResult<()> {
    let Some(collection) = event.ns.as_ref().and_then(|ns| ns.coll.clone()) else {
        return Ok(());
    };
    let Some(full) = event.full_document.clone() else {
        return Ok(());
    };
    let Some(source_token) = event_token(event) else {
        return Ok(());
    };

    let Ok(tenant_id) = full.get_str("tenant_id") else {
        return Ok(());
    };
    if tenant_id == DENY_TENANT {
        return Ok(());
    }
    if only_derived_changed(&collection, event) {
        return Ok(());
    }
    let tenant = Tenant::id(tenant_id)?;
    if collection == TOMBSTONES {
        return with_tenant(tenant, publish_tombstone(db, full, &source_token)).await;
    }
    with_tenant(tenant, publish(db, &collection, full, &source_token)).await
}

/// True for an update that only moved derived columns (a sale's stock
/// change, a recomputed invoice total). Every replica recomputes those from
/// its own ledgers and never takes them from a payload, so publishing the row
/// would only wake every device for nothing.
fn only_derived_changed(collection: &str, event: &ChangeStreamEvent<Document>) -> bool {
    let Some(spec) = ordered().find(|s| s.table == collection) else {
        return false;
    };
    let Some(update) = event.update_description.as_ref() else {
        return false;
    };
    !spec.derived.is_empty()
        && update.removed_fields.is_empty()
        && !update.updated_fields.is_empty()
        && update
            .updated_fields
            .keys()
            .all(|field| spec.derived.contains(&field.as_str()))
}

/// Publishes a hard-delete marker as a delete of its resource.
async fn publish_tombstone(db: &Db, marker: Document, source_token: &str) -> AppResult<()> {
    let (Ok(table), Ok(key)) = (marker.get_str("table"), marker.get_str("key")) else {
        return Ok(());
    };
    let Some(spec) = ordered().find(|s| s.table == table && s.phase == Phase::P3a) else {
        return Ok(());
    };
    let device = marker
        .get_str("updated_by_device")
        .unwrap_or(crate::core::sync_origin::CLOUD_ORIGIN)
        .to_string();
    let deleted_at = marker
        .get_datetime("deleted_at")
        .copied()
        .unwrap_or_else(|_| BsonDateTime::now());
    let row = doc! {
        "resource": spec.name,
        "key": key,
        "op": "delete",
        "version": 1_i64,
        "updated_at": deleted_at,
        "device_id": &device,
        "origin_device_id": &device,
        "received_at": BsonDateTime::now(),
        "source_token": source_token,
    };
    insert_change(db, row).await
}

/// Appends a change with the next `seq`. A replayed event collides on the
/// unique `source_token` and is dropped; its unused `seq` only leaves a gap,
/// which pulls (`seq > cursor`) never notice.
async fn insert_change(db: &Db, mut row: Document) -> AppResult<()> {
    let seq = cloud_store::allocate_seq(db).await?;
    row.insert("seq", seq);
    match coll(db, CHANGES)?.insert_one(row).await {
        Ok(_) => Ok(()),
        Err(err) if err.to_string().contains("E11000") => Ok(()),
        Err(err) => Err(err.into()),
    }
}

async fn publish(db: &Db, collection: &str, full: Document, source_token: &str) -> AppResult<()> {
    let (table, doc_for_change) = (collection, full);
    let Some(spec) = ordered().find(|s| s.table == table && s.phase == Phase::P3a) else {
        return Ok(());
    };

    let key = doc_for_change
        .get_str("key")
        .unwrap_or_default()
        .to_string();
    if key.is_empty() {
        return Ok(());
    }
    let deleted = doc_for_change
        .get("deleted_at")
        .is_some_and(|v| !matches!(v, Bson::Null));
    let payload: Option<Bson> = if deleted {
        None
    } else {
        let value = hydrate(db, spec.name, vec![doc_for_change.clone()])
            .await?
            .into_iter()
            .next();
        match value {
            Some(v) => Some(
                bson::serialize_to_bson(&v)
                    .map_err(|e| AppError::internal(format!("cannot store payload: {e}")))?,
            ),
            None => return Ok(()),
        }
    };

    let device = doc_for_change
        .get_str("updated_by_device")
        .ok()
        .map(str::to_string);
    let mut row = doc! {
        "resource": spec.name,
        "key": &key,
        "op": if deleted { "delete" } else { "upsert" },
        "version": crate::modules::sync::apply_mongo::num_i64(&doc_for_change, "version").unwrap_or(1),
        "updated_at": doc_for_change
            .get_datetime("updated_at")
            .copied()
            .unwrap_or_else(|_| BsonDateTime::now()),
        "device_id": device.clone().unwrap_or_else(|| "cloud".to_string()),
        "received_at": BsonDateTime::now(),
        "source_token": source_token,
    };
    if let Some(payload) = payload {
        row.insert("payload", payload);
    }
    if let Some(device) = device {
        row.insert("origin_device_id", device);
    }
    insert_change(db, row).await
}
