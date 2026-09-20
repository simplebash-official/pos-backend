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

use std::time::{Duration, Instant};

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
        cloud_store::{self, CHANGES, coll},
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
const POLL_IDLE: Duration = Duration::from_millis(250);
const COMPACT_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Runs the consumer forever. Spawn once per process when multi-tenant.
pub async fn run_consumer(db: Db) -> ! {
    let holder = generate_id("cns");
    let mut last_compaction = Instant::now();
    loop {
        match try_acquire_lease(&db, &holder).await {
            Ok(true) => {
                tracing::info!(%holder, "sync change consumer acquired the lease");
                if let Err(err) = consume(&db, &holder, &mut last_compaction).await {
                    tracing::warn!(%err, "sync change consumer stopped; will retry");
                }
            }
            Ok(false) => {}
            Err(err) => tracing::warn!(%err, "sync consumer lease check failed"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
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
            doc! { "$set": { "resume_token": value } },
        )
        .await?;
    Ok(())
}

/// Watches the database until the lease is lost or the stream fails.
async fn consume(db: &Db, holder: &str, last_compaction: &mut Instant) -> AppResult<()> {
    let raw = db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .unscoped()
        .clone();

    let mut tables: Vec<&str> = ordered()
        .filter(|s| s.phase == Phase::P3a)
        .map(|s| s.table)
        .collect();
    // A subcategory change is published as a change of its parent category.
    tables.push("subcategories");

    let pipeline = vec![doc! { "$match": {
        "ns.coll": { "$in": tables },
        "operationType": { "$in": ["insert", "update", "replace"] },
    }}];
    let mut watch = raw
        .watch()
        .pipeline(pipeline)
        .full_document(FullDocumentType::UpdateLookup);
    if let Some(token) = load_resume_token(db).await? {
        watch = watch.start_after(token);
    }
    let mut stream = watch.await?;

    let mut since_persist = 0u32;
    let mut last_persist = Instant::now();
    let mut last_renew = Instant::now();

    loop {
        match stream.next_if_any().await? {
            Some(event) => {
                if let Err(err) = process_event(db, event).await {
                    // One bad event must not wedge the whole feed.
                    tracing::error!(%err, "failed to capture a change event");
                }
                since_persist += 1;
            }
            None => tokio::time::sleep(POLL_IDLE).await,
        }

        if last_renew.elapsed() >= RENEW_EVERY {
            if !renew_lease(db, holder).await? {
                tracing::warn!("sync consumer lost its lease");
                return Ok(());
            }
            last_renew = Instant::now();
        }
        if since_persist >= 100 || (since_persist > 0 && last_persist.elapsed() >= PERSIST_EVERY) {
            if let Some(token) = stream.resume_token() {
                persist_resume_token(db, holder, &token).await?;
            }
            since_persist = 0;
            last_persist = Instant::now();
        }
        if last_compaction.elapsed() >= COMPACT_EVERY {
            *last_compaction = Instant::now();
            match compaction::compact_all(db, compaction::RETENTION_DAYS).await {
                Ok(n) => tracing::info!(dropped = n, "sync change log compacted"),
                Err(err) => tracing::warn!(%err, "sync change log compaction failed"),
            }
        }
    }
}

/// Publishes one database event to its tenant's change log.
async fn process_event(db: &Db, event: ChangeStreamEvent<Document>) -> AppResult<()> {
    let Some(collection) = event.ns.as_ref().and_then(|ns| ns.coll.clone()) else {
        return Ok(());
    };
    let Some(full) = event.full_document else {
        return Ok(());
    };
    // The event id is unique per event: the natural de-duplication key.
    let source_token = bson::serialize_to_bson(&event.id)
        .ok()
        .and_then(|b| b.as_document().and_then(|d| d.get_str("_data").ok().map(str::to_string)));
    let Some(source_token) = source_token else {
        return Ok(());
    };

    let Ok(tenant_id) = full.get_str("tenant_id") else {
        return Ok(());
    };
    if tenant_id == DENY_TENANT {
        return Ok(());
    }
    let tenant = Tenant::id(tenant_id)?;
    with_tenant(tenant, publish(db, &collection, full, &source_token)).await
}

async fn publish(db: &Db, collection: &str, full: Document, source_token: &str) -> AppResult<()> {
    let changes = coll(db, CHANGES)?;
    if changes
        .find_one(doc! { "source_token": source_token })
        .await?
        .is_some()
    {
        return Ok(());
    }

    // A subcategory is not a wire resource: it changed, so its category did.
    let (table, doc_for_change) = if collection == "subcategories" {
        let Ok(parent_key) = full.get_str("category_key") else {
            return Ok(());
        };
        let Some(parent) = coll(db, "categories")?
            .find_one(doc! { "key": parent_key })
            .await?
        else {
            return Ok(());
        };
        ("categories", parent)
    } else {
        (collection, full)
    };
    let Some(spec) = ordered().find(|s| s.table == table && s.phase == Phase::P3a) else {
        return Ok(());
    };

    let key = doc_for_change.get_str("key").unwrap_or_default().to_string();
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
    let seq = cloud_store::allocate_seq(db).await?;
    let mut row = doc! {
        "seq": seq,
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
    changes.insert_one(row).await?;
    Ok(())
}
