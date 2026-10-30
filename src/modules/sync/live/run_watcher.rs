use super::live_event_of::live_event_of;
use super::publish::publish;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};

use crate::modules::sync::cloud_store::CHANGES;

use mongodb::bson::{Document, doc};

use std::time::Duration;

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
