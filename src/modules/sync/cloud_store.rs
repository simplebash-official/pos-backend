// Storage layer of the cloud (MongoDB) side of sync v2: the tenant-scoped
// collections behind push/pull/snapshot and the small pieces of state around
// them (per-tenant sequence, device registry, replayable batch acks,
// conflicts). Every accessor goes through the scoped handle, so a call inside
// `core::tenancy::with_tenant` can only ever see its own tenant's rows.
//
// It deliberately owns no business rules: what a change means lives in
// `apply_mongo`/`push`, and ordering lives in the change-stream consumer.

use mongodb::{
    bson::{self, DateTime as BsonDateTime, Document, doc},
    options::ReturnDocument,
};

use crate::{
    clients::{db::Db, tenant_db::ScopedCollection},
    core::{
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::sync_v2::{ChangeAck, NewConflict},
};

pub(crate) const CHANGES: &str = "sync_changes";
pub(crate) const META: &str = "sync_meta";
pub(crate) const DEVICES: &str = "sync_device_state";
pub(crate) const BATCHES: &str = "sync_batches";
pub(crate) const CONFLICTS: &str = "sync_conflicts";

/// The tenant-scoped collection `name` (ambient tenant of the running request).
pub(crate) fn coll(db: &Db, name: &str) -> AppResult<ScopedCollection<Document>> {
    Ok(db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .collection::<Document>(name))
}

/// Allocates the next per-tenant sequence number (the pull cursor). Only the
/// single change-stream consumer calls this, so seq order equals commit order.
pub(crate) async fn allocate_seq(db: &Db) -> AppResult<i64> {
    let updated = coll(db, META)?
        .find_one_and_update(
            doc! { "kind": "meta" },
            doc! { "$inc": { "last_seq": 1_i64 } },
        )
        .upsert(true)
        .return_document(ReturnDocument::After)
        .await?;
    updated
        .and_then(|d| d.get_i64("last_seq").ok())
        .ok_or_else(|| AppError::internal("failed to allocate a sync sequence number"))
}

async fn meta_i64(db: &Db, field: &str) -> AppResult<i64> {
    Ok(coll(db, META)?
        .find_one(doc! { "kind": "meta" })
        .await?
        .and_then(|d| d.get_i64(field).ok())
        .unwrap_or(0))
}

/// Highest sequence number allocated so far for the tenant.
pub(crate) async fn current_seq(db: &Db) -> AppResult<i64> {
    meta_i64(db, "last_seq").await
}

/// Changes at or below this seq were compacted away; a puller behind it must
/// bootstrap from a snapshot.
pub(crate) async fn compacted_through(db: &Db) -> AppResult<i64> {
    meta_i64(db, "compacted_through_seq").await
}

pub(crate) async fn set_compacted_through(db: &Db, seq: i64) -> AppResult<()> {
    coll(db, META)?
        .update_one(
            doc! { "kind": "meta" },
            doc! { "$max": { "compacted_through_seq": seq } },
        )
        .upsert(true)
        .await?;
    Ok(())
}

/// Whether the device is registered for the tenant and not revoked. A device
/// seen for the first time is registered on the spot (token issuance already
/// authenticated it), so `Ok(true)` also covers "new".
pub(crate) async fn ensure_device_active(db: &Db, device_id: &str) -> AppResult<bool> {
    let state = coll(db, DEVICES)?
        .find_one_and_update(
            doc! { "device_id": device_id },
            doc! { "$setOnInsert": {
                "device_id": device_id,
                "revoked": false,
                "last_outbox_seq": 0_i64,
                "cloud_cursor": 0_i64,
                "registered_at": BsonDateTime::now(),
            }},
        )
        .upsert(true)
        .return_document(ReturnDocument::After)
        .await?;
    Ok(!state
        .and_then(|d| d.get_bool("revoked").ok())
        .unwrap_or(false))
}

pub(crate) async fn touch_device(
    db: &Db,
    device_id: &str,
    outbox_seq: Option<i64>,
    batch_id: Option<&str>,
    cloud_cursor: Option<i64>,
) -> AppResult<()> {
    let mut set = doc! { "last_seen": BsonDateTime::now() };
    if let Some(batch_id) = batch_id {
        set.insert("last_batch_id", batch_id);
    }
    let mut update = doc! { "$set": set };
    let mut max = Document::new();
    if let Some(seq) = outbox_seq {
        max.insert("last_outbox_seq", seq);
    }
    if let Some(cursor) = cloud_cursor {
        max.insert("cloud_cursor", cursor);
    }
    if !max.is_empty() {
        update.insert("$max", max);
    }
    coll(db, DEVICES)?
        .update_one(doc! { "device_id": device_id }, update)
        .upsert(true)
        .await?;
    Ok(())
}

/// Acks recorded for an already-processed batch, if any (push retry).
pub(crate) async fn find_batch_acks(
    db: &Db,
    device_id: &str,
    batch_id: &str,
) -> AppResult<Option<Vec<ChangeAck>>> {
    let Some(stored) = coll(db, BATCHES)?
        .find_one(doc! { "device_id": device_id, "batch_id": batch_id })
        .await?
    else {
        return Ok(None);
    };
    let acks = stored
        .get("acks")
        .cloned()
        .map(bson::deserialize_from_bson::<Vec<ChangeAck>>)
        .transpose()
        .map_err(|e| AppError::internal(format!("stored batch acks are unreadable: {e}")))?;
    Ok(acks)
}

pub(crate) async fn store_batch_acks(
    db: &Db,
    device_id: &str,
    batch_id: &str,
    acks: &[ChangeAck],
) -> AppResult<()> {
    let acks = bson::serialize_to_bson(acks)
        .map_err(|e| AppError::internal(format!("cannot store batch acks: {e}")))?;
    // Upsert: a retry that raced the first attempt overwrites with the same acks.
    coll(db, BATCHES)?
        .update_one(
            doc! { "device_id": device_id, "batch_id": batch_id },
            doc! { "$set": { "acks": acks, "at": BsonDateTime::now() } },
        )
        .upsert(true)
        .await?;
    Ok(())
}

/// Persists conflicts raised while applying a batch.
pub(crate) async fn record_conflicts(
    db: &Db,
    device_id: &str,
    conflicts: &[NewConflict],
) -> AppResult<()> {
    if conflicts.is_empty() {
        return Ok(());
    }
    let collection = coll(db, CONFLICTS)?;
    for conflict in conflicts {
        let detail = bson::serialize_to_bson(&conflict.detail)
            .map_err(|e| AppError::internal(format!("cannot store conflict detail: {e}")))?;
        collection
            .insert_one(doc! {
                "key": generate_id("scf"),
                "kind": conflict.kind.as_str(),
                "resource": &conflict.resource,
                "entity_key": &conflict.entity_key,
                "detail": detail,
                "device_id": device_id,
                "detected_at": BsonDateTime::now(),
            })
            .await?;
    }
    Ok(())
}
