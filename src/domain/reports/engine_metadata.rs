use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Metadata describing engine query execution and caching status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EngineMetadata {
    /// Time taken to process the request on the server in milliseconds.
    pub execution_time_ms: u64,
    /// Whether the response was served from the in-memory cache.
    pub cache_hit: bool,
    /// The timestamp when the data was computed / cached (UTC).
    pub cached_at: DateTime<Utc>,
    /// Cache TTL remaining or status description.
    pub cache_status: String,
}
