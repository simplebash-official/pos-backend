use super::syncable::SYNCABLE;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::domain::sync::{ResourceSyncStatus, SyncStatusResponse};

use crate::modules::sync::cursor::encode_cursor;
use crate::modules::sync::repository;
use chrono::Utc;

use std::collections::HashMap;

pub async fn get_sync_status(db: &Db) -> AppResult<SyncStatusResponse> {
    let mut resources = HashMap::new();

    for (resource_name, collection_name) in SYNCABLE {
        if let Some((updated_at, key)) =
            repository::fetch_latest_change_marker(db, collection_name).await?
        {
            let timestamp = updated_at.to_chrono();
            resources.insert(
                resource_name.to_string(),
                ResourceSyncStatus {
                    last_updated_at: timestamp,
                    cursor: encode_cursor(timestamp, &key),
                },
            );
        }
    }

    Ok(SyncStatusResponse {
        server_time: Utc::now(),
        resources,
        setup_completed: false,
        sample_data_loaded: false,
    })
}
