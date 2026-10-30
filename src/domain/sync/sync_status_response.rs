use super::resource_sync_status::ResourceSyncStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Response payload containing the latest sync watermark for all resources.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusResponse {
    /// Current server timestamp in UTC.
    pub server_time: DateTime<Utc>,
    /// Map of resource name to current sync watermark status.
    pub resources: HashMap<String, ResourceSyncStatus>,
    /// Cloud (multi-tenant) only: the shop has finished its own first-time setup
    /// (the demo-data vs clean-data choice). Always false on a single-shop install.
    #[serde(default)]
    pub setup_completed: bool,
    /// Cloud only: that setup loaded the demo data.
    #[serde(default)]
    pub sample_data_loaded: bool,
}
