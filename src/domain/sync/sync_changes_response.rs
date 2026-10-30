use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Response payload containing change deltas across requested resources.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncChangesResponse {
    /// Current server timestamp in UTC.
    pub server_time: DateTime<Utc>,
    /// Map of resource name (e.g. "products") to its change payload.
    pub changes: HashMap<String, serde_json::Value>,
}
