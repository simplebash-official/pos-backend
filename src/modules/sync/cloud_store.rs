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
/// Markers for rows a cloud write removed outright (see `record_hard_deletes`).
pub(crate) const TOMBSTONES: &str = "sync_tombstones";

/// The tenant-scoped collection `name` (ambient tenant of the running request).
pub(crate) fn coll(db: &Db, name: &str) -> AppResult<ScopedCollection<Document>> {
    Ok(db
        .as_mongo()
        .ok_or_else(|| AppError::internal("cloud sync requires MongoDB"))?
        .collection::<Document>(name))
}

/// Records that rows of `table` were removed outright (not soft-deleted), so
/// the change-stream consumer can publish them as deletes: a removed document
/// leaves nothing behind for it to read. A no-op outside the multi-tenant
/// cloud (no change log there) and for an empty `keys`.
pub(crate) async fn record_hard_deletes(db: &Db, table: &str, keys: &[String]) -> AppResult<()> {
    let Some(mongo) = db.as_mongo() else {
        return Ok(());
    };
    if keys.is_empty() || !mongo.effective_tenant().is_scoped() {
        return Ok(());
    }
    let origin = crate::core::sync_origin::current_origin()
        .map(|o| o.to_string())
        .unwrap_or_else(|| crate::core::sync_origin::CLOUD_ORIGIN.to_string());
    let tombstones = mongo.collection::<Document>(TOMBSTONES);
    let now = BsonDateTime::now();
    for key in keys {
        tombstones
            .insert_one(doc! {
                "table": table,
                "key": key,
                "deleted_at": now,
                "updated_by_device": &origin,
            })
            .await?;
    }
    Ok(())
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
    // A plain read on the hot path (every pull); the upsert below only runs
    // the first time a device shows up.
    if let Some(known) = coll(db, DEVICES)?
        .find_one(doc! { "device_id": device_id })
        .await?
    {
        return Ok(!known.get_bool("revoked").unwrap_or(false));
    }
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

/// The oldest desktop app that may sync with this cloud: 1.14.0 is the first
/// release whose data has stock batches (prices per delivery; 1.13.0 shipped the
/// new screens on the old backend). An older app
/// would push stock moves tied to no delivery and pull products it has no
/// price for, so it is held back until it updates.
pub(crate) const MIN_DESKTOP_APP_VERSION: (u64, u64, u64) = (1, 14, 0);

/// Refusal for a desktop older than [`MIN_DESKTOP_APP_VERSION`]. A 503 on
/// purpose: older apps retry a 5xx later and keep their waiting changes,
/// whereas any other 4xx makes them drop the refused change for good.
fn update_required() -> AppError {
    AppError::custom(
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "APP_UPDATE_REQUIRED",
        "This SimpleBash app is too old to sync. Update it under Settings > Updates. \
         Everything you do on this computer is kept and syncs after the update.",
    )
}

/// `"1.13.0"` (a `-suffix` such as `-dev` is ignored) as comparable numbers;
/// `None` for anything that is not three dot-separated numbers.
fn parse_app_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim().split('-').next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}

/// Records the app version a device registered with, refusing one too old
/// to sync (see [`MIN_DESKTOP_APP_VERSION`]).
pub(crate) async fn record_device_app_version(
    db: &Db,
    device_id: &str,
    app_version: &str,
) -> AppResult<()> {
    if parse_app_version(app_version).is_none_or(|v| v < MIN_DESKTOP_APP_VERSION) {
        return Err(update_required());
    }
    coll(db, DEVICES)?
        .update_one(
            doc! { "device_id": device_id },
            doc! { "$set": { "app_version": app_version } },
        )
        .await?;
    Ok(())
}

/// Refuses a device whose registered app is older than
/// [`MIN_DESKTOP_APP_VERSION`], or that never registered a version at all
/// (every app older than the cloud's version check).
pub(crate) async fn ensure_device_app_current(db: &Db, device_id: &str) -> AppResult<()> {
    let version = coll(db, DEVICES)?
        .find_one(doc! { "device_id": device_id })
        .await?
        .and_then(|d| d.get_str("app_version").ok().and_then(parse_app_version));
    match version {
        Some(v) if v >= MIN_DESKTOP_APP_VERSION => Ok(()),
        _ => Err(update_required()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_versions_parse_and_compare_numerically() {
        assert_eq!(parse_app_version("1.13.0"), Some((1, 13, 0)));
        assert_eq!(parse_app_version("1.13.2-dev"), Some((1, 13, 2)));
        assert!(parse_app_version("1.9.0").unwrap() < MIN_DESKTOP_APP_VERSION);
        assert!(parse_app_version("1.12.0").unwrap() < MIN_DESKTOP_APP_VERSION);
        assert!(parse_app_version("2.0.0").unwrap() > MIN_DESKTOP_APP_VERSION);
        for bad in ["", "1.13", "1.13.0.1", "v1.13.0", "one.two.three"] {
            assert_eq!(parse_app_version(bad), None, "{bad}");
        }
    }
}
