// `GET /api/sync/pull`: the ordered change feed a device reads. The cursor is
// the tenant's integer `seq` (assigned by the single change-stream consumer),
// not a timestamp, so device clocks never affect what a puller sees.
//
// A device does not receive its own writes back: changes whose
// `origin_device_id` is the caller are skipped, but the cursor still advances
// past them, so a device that only ever pushes does not re-scan the same range.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::http::StatusCode;
use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use serde_json::Value;

use crate::{
    clients::db::Db,
    core::{
        error::{AppError, AppResult},
        tenancy::current_tenant_id,
    },
    domain::sync_v2::{ChangeOp, ChangeRecord, PullResponse, PulledChange},
    modules::sync::cloud_store::{self, CHANGES, coll},
};

pub(crate) const DEFAULT_PULL_LIMIT: i64 = 200;
pub(crate) const MAX_PULL_LIMIT: i64 = 500;

/// Reads one stored `sync_changes` row back into its wire form.
pub(crate) fn change_from_doc(row: &Document) -> AppResult<PulledChange> {
    let text = |field: &str| -> AppResult<String> {
        row.get_str(field)
            .map(str::to_string)
            .map_err(|_| AppError::internal(format!("stored change has no '{field}'")))
    };
    let op = match row.get_str("op") {
        Ok("delete") => ChangeOp::Delete,
        _ => ChangeOp::Upsert,
    };
    let updated_at = row
        .get_datetime("updated_at")
        .map(|d| d.to_chrono())
        .map_err(|_| AppError::internal("stored change has no 'updated_at'"))?;
    let payload = match row.get("payload") {
        Some(mongodb::bson::Bson::Document(p)) => Some(
            serde_json::to_value(p)
                .map_err(|e| AppError::internal(format!("stored payload is unreadable: {e}")))?,
        ),
        _ => None,
    };
    Ok(PulledChange {
        seq: row.get_i64("seq").unwrap_or_default(),
        origin_device_id: row.get_str("origin_device_id").ok().map(str::to_string),
        record: ChangeRecord {
            resource: text("resource")?,
            key: text("key")?,
            op,
            version: crate::modules::sync::apply_mongo::num_i64(row, "version").unwrap_or(1),
            updated_at,
            device_id: row.get_str("device_id").unwrap_or("cloud").to_string(),
            payload: payload.filter(|v: &Value| v.is_object()),
        },
    })
}

/// How often a pull records the device's last-seen time and cursor. Those are
/// diagnostics only; realtime pulls arrive many times a minute and should not
/// each cost a database write.
const TOUCH_EVERY: Duration = Duration::from_secs(60);

/// Whether this pull should record the device as seen (per tenant + device).
fn touch_due(device_id: &str) -> bool {
    static LAST: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let key = format!("{}|{device_id}", current_tenant_id().unwrap_or_default());
    let mut last = LAST
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    match last.get(&key) {
        Some(at) if now.duration_since(*at) < TOUCH_EVERY => false,
        _ => {
            last.insert(key, now);
            true
        }
    }
}

/// Changes after `since` for `device_id`. `410 CURSOR_EXPIRED` when `since`
/// predates compaction: the caller must bootstrap from a snapshot instead.
pub async fn pull(
    db: &Db,
    device_id: &str,
    since: i64,
    limit: Option<i64>,
) -> AppResult<PullResponse> {
    let limit = limit.unwrap_or(DEFAULT_PULL_LIMIT).clamp(1, MAX_PULL_LIMIT);
    if since < 0 {
        return Err(AppError::validation("since must not be negative"));
    }
    if since < cloud_store::compacted_through(db).await? {
        return Err(AppError::custom(
            StatusCode::GONE,
            "CURSOR_EXPIRED",
            "The change history behind this cursor was compacted; bootstrap from a snapshot",
        ));
    }
    if !cloud_store::ensure_device_active(db, device_id).await? {
        return Err(AppError::forbidden_with_code(
            "This device has been revoked",
            "DEVICE_REVOKED",
        ));
    }

    let mut cursor = coll(db, CHANGES)?
        .find(doc! { "seq": { "$gt": since } })
        .sort(doc! { "seq": 1 })
        .limit(limit + 1)
        .await?;
    let mut rows = Vec::new();
    while let Some(row) = cursor.try_next().await? {
        rows.push(row);
    }

    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    // The cursor advances over everything scanned, echoes included.
    let next_seq = rows
        .last()
        .and_then(|r| r.get_i64("seq").ok())
        .unwrap_or(since);

    let mut changes = Vec::with_capacity(rows.len());
    for row in &rows {
        if row.get_str("origin_device_id").ok() == Some(device_id) {
            continue;
        }
        changes.push(change_from_doc(row)?);
    }

    if touch_due(device_id) {
        cloud_store::touch_device(db, device_id, None, None, Some(next_seq)).await?;
    }
    Ok(PullResponse {
        server_time: Utc::now(),
        changes,
        next_seq,
        has_more,
    })
}
