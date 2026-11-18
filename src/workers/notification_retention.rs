use crate::clients::db::Db;
use crate::modules::notifications::service;
use std::time::Duration;

/// How often old notifications are removed.
const PURGE_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Removes notifications past the retention window once at start and then daily, forever.
/// A failed run is logged and retried at the next turn: nothing depends on it finishing.
pub async fn run_notification_retention(db: Db) -> ! {
    let mut ticks = tokio::time::interval(PURGE_EVERY);
    loop {
        ticks.tick().await;
        match service::purge_expired(&db).await {
            Ok(0) => {}
            Ok(removed) => tracing::info!(removed, "old notifications removed"),
            Err(err) => tracing::warn!(%err, "notification retention run failed"),
        }
    }
}
