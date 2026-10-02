// `POST /api/sync/push`: a device sends its outbox changes; the cloud merges
// each one with the pure engine (`core::sync_merge::decide`), writes the winners
// through `apply_mongo`, recomputes derived fields once for the whole batch and
// answers with a per-change ack. Retrying a batch is safe twice over: the
// recorded acks of a `(deviceId, batchId)` are replayed, and re-applying an
// identical change decides `Duplicate` anyway.
//
// The push path never writes `sync_changes`. The change-stream consumer does
// (`cloud_capture`), so web-originated and device-originated writes reach the
// pull feed through the same single, ordered path.

use chrono::Utc;
use serde_json::Value;

use crate::{
    clients::db::Db,
    core::{
        error::{AppError, AppResult},
        sync_merge::{Decision, IncomingChange, META_KEYS, clamp_updated_at, decide},
    },
    domain::sync_v2::{
        AckStatus, ChangeAck, ChangeOp, ChangeRecord, ConflictKind, DerivedScope, NewConflict,
        PushRequest, PushResponse,
    },
    modules::sync::{
        apply_mongo::{
            ExistingRow, WriteOutcome, apply_delete, apply_lifecycle, apply_upsert, load_existing,
        },
        cloud_store,
        derived_mongo::recompute_derived,
        resources::{ResourceSpec, spec as resource_spec, without_secrets},
        service::hydrate,
    },
};

/// Most changes one push may carry.
pub(crate) const MAX_PUSH_CHANGES: usize = 500;

fn ack(record: &ChangeRecord, status: AckStatus, reason: Option<&str>, clamped: bool) -> ChangeAck {
    ChangeAck {
        key: record.key.clone(),
        resource: record.resource.clone(),
        status,
        reason: reason.map(str::to_string),
        clamped,
    }
}

/// True when the two payloads differ in something other than write metadata.
fn differs_ignoring_meta(a: &Value, b: &Value) -> bool {
    let strip = |v: &Value| -> Value {
        match v {
            Value::Object(map) => Value::Object(
                map.iter()
                    .filter(|(k, _)| !META_KEYS.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
            other => other.clone(),
        }
    };
    strip(a) != strip(b)
}

/// The record that lost last-writer-wins, kept for review.
async fn lww_loser_conflict(
    db: &Db,
    spec: &ResourceSpec,
    record: &ChangeRecord,
    existing: &ExistingRow,
) -> AppResult<Option<NewConflict>> {
    let winning = hydrate(db, spec.name, vec![existing.doc.clone()])
        .await?
        .into_iter()
        .next();
    // A no-op edit (same content, different timestamp) is not a conflict.
    if let (Some(local), Some(incoming)) = (winning.as_ref(), record.payload.as_ref())
        && !differs_ignoring_meta(local, incoming)
    {
        return Ok(None);
    }
    Ok(Some(NewConflict {
        kind: ConflictKind::LwwLoser,
        resource: spec.name.to_string(),
        entity_key: record.key.clone(),
        detail: serde_json::json!({
            // Never persist a secret (the users password hash) outside its own table.
            "losingPayload": record.payload.as_ref().map(|p| without_secrets(spec.name, p)),
            "winningDeviceId": existing.meta.device_id,
            "winningUpdatedAt": existing.meta.updated_at_ms,
        }),
    }))
}

/// Merges and applies one change; returns its ack. Business-level refusals
/// become `Rejected` acks, storage failures bubble up.
async fn process_change(
    db: &Db,
    device_id: &str,
    record: &ChangeRecord,
    now_ms: i64,
    conflicts: &mut Vec<NewConflict>,
    scope: &mut DerivedScope,
) -> AppResult<ChangeAck> {
    let Some(spec) = resource_spec(&record.resource) else {
        return Ok(ack(
            record,
            AckStatus::Rejected,
            Some("UNKNOWN_RESOURCE"),
            false,
        ));
    };
    let (effective_ms, clamped) = clamp_updated_at(record.updated_at.timestamp_millis(), now_ms);
    let existing = load_existing(db, spec, &record.key).await?;
    let incoming = IncomingChange {
        record,
        updated_at_ms: effective_ms,
        clamped,
    };
    let decision = decide(spec, existing.as_ref().map(|e| &e.meta), &incoming);

    let applied = |reason: Option<&str>| ack(record, AckStatus::Applied, reason, clamped);
    let clamp_reason = clamped.then_some("CLOCK_CLAMPED");

    let result: AppResult<ChangeAck> = async {
        match decision {
            Decision::Duplicate => Ok(ack(record, AckStatus::Duplicate, None, clamped)),
            Decision::Reject { reason } => {
                Ok(ack(record, AckStatus::Rejected, Some(reason), false))
            }
            Decision::KeepLocal { .. } => {
                if let Some(existing) = existing.as_ref()
                    && let Some(conflict) = lww_loser_conflict(db, spec, record, existing).await?
                {
                    conflicts.push(conflict);
                    return Ok(ack(record, AckStatus::Conflict, Some("LWW_LOSER"), clamped));
                }
                // Lost on timestamp but carried no new content: nothing to review.
                Ok(ack(record, AckStatus::Duplicate, None, clamped))
            }
            Decision::MergeLifecycle => {
                let Some(existing) = existing.as_ref() else {
                    return Ok(ack(
                        record,
                        AckStatus::Rejected,
                        Some("INVALID_PAYLOAD"),
                        false,
                    ));
                };
                match apply_lifecycle(db, spec, record, effective_ms, device_id, existing, scope)
                    .await?
                {
                    WriteOutcome::Written => Ok(applied(clamp_reason)),
                    WriteOutcome::Unchanged => Ok(ack(record, AckStatus::Duplicate, None, clamped)),
                    WriteOutcome::Rejected(reason) => {
                        Ok(ack(record, AckStatus::Rejected, Some(reason), false))
                    }
                }
            }
            Decision::Apply => {
                let before = conflicts.len();
                let outcome = match record.op {
                    ChangeOp::Upsert => {
                        apply_upsert(
                            db,
                            spec,
                            record,
                            effective_ms,
                            device_id,
                            existing.as_ref(),
                            (&mut *conflicts, &mut *scope),
                        )
                        .await?
                    }
                    ChangeOp::Delete => {
                        apply_delete(
                            db,
                            spec,
                            record,
                            effective_ms,
                            device_id,
                            existing.as_ref(),
                            scope,
                        )
                        .await?
                    }
                };
                if let WriteOutcome::Rejected(reason) = outcome {
                    return Ok(ack(record, AckStatus::Rejected, Some(reason), false));
                }
                let serial_conflict = conflicts[before..]
                    .iter()
                    .any(|c| c.kind == ConflictKind::SerialDoubleSold);
                Ok(applied(
                    serial_conflict
                        .then_some("SERIAL_DOUBLE_SOLD")
                        .or(clamp_reason),
                ))
            }
        }
    }
    .await;

    match result {
        Err(AppError::Validation { .. }) => Ok(ack(
            record,
            AckStatus::Rejected,
            Some("INVALID_PAYLOAD"),
            false,
        )),
        other => other,
    }
}

/// Processes a push batch for `device_id` (already authenticated from its
/// device token). Runs inside the caller's ambient tenant.
pub async fn push(db: &Db, device_id: &str, req: PushRequest) -> AppResult<PushResponse> {
    if req.device_id != device_id {
        return Err(AppError::forbidden_with_code(
            "Push deviceId does not match the device token",
            "DEVICE_MISMATCH",
        ));
    }
    if req.changes.len() > MAX_PUSH_CHANGES {
        return Err(AppError::validation(format!(
            "A push carries at most {MAX_PUSH_CHANGES} changes"
        )));
    }
    if !cloud_store::ensure_device_active(db, device_id).await? {
        return Err(AppError::forbidden_with_code(
            "This device has been revoked",
            "DEVICE_REVOKED",
        ));
    }

    // A retried batch gets the same answer without touching any data.
    if let Some(acks) = cloud_store::find_batch_acks(db, device_id, &req.batch_id).await? {
        return Ok(PushResponse {
            server_time: Utc::now(),
            acks,
            server_seq: cloud_store::current_seq(db).await?,
        });
    }

    // Parents before children; original request order breaks ties.
    let mut order: Vec<usize> = (0..req.changes.len()).collect();
    order.sort_by_key(|&i| {
        let r = &req.changes[i];
        (resource_spec(&r.resource).map_or(u8::MAX, |s| s.order), i)
    });

    let now_ms = Utc::now().timestamp_millis();
    let mut conflicts = Vec::new();
    let mut scope = DerivedScope::default();
    let mut acks: Vec<Option<ChangeAck>> = vec![None; req.changes.len()];
    for i in order {
        acks[i] = Some(
            process_change(
                db,
                device_id,
                &req.changes[i],
                now_ms,
                &mut conflicts,
                &mut scope,
            )
            .await?,
        );
    }
    let acks: Vec<ChangeAck> = acks.into_iter().flatten().collect();

    if !scope.is_empty() {
        conflicts.extend(recompute_derived(db, &scope).await?);
    }
    cloud_store::record_conflicts(db, device_id, &conflicts).await?;
    cloud_store::store_batch_acks(db, device_id, &req.batch_id, &acks).await?;
    cloud_store::touch_device(
        db,
        device_id,
        req.outbox_seqs.iter().copied().max(),
        Some(&req.batch_id),
        None,
    )
    .await?;

    Ok(PushResponse {
        server_time: Utc::now(),
        acks,
        server_seq: cloud_store::current_seq(db).await?,
    })
}
