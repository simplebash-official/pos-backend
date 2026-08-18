use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for fetching incremental change deltas.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SyncChangesQuery {
    /// JSON string of map `{"products": "...", "suppliers": "..."}` or comma-delimited `products=...,suppliers=...`
    pub cursors: Option<String>,
    /// Shorthand single cursor when syncing one resource
    pub since: Option<String>,
    /// Limit per resource type (1..500, default 500). `0` asks for a
    /// cursor-only response: no items are returned, but `nextCursor` points
    /// at the newest row, which is what a client needs to start syncing
    /// incrementally after taking a snapshot by some other route.
    pub limit: Option<u64>,
    /// Comma-separated list of resources to sync (defaults to all syncable resources)
    pub resources: Option<String>,
}

/// Change delta container for a specific entity type.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceChanges<T: Serialize> {
    /// Indicates if a full snapshot is returned (e.g. initial sync).
    pub full: bool,
    /// List of created or updated records.
    pub items: Vec<T>,
    /// List of keys for records deleted since the cursor.
    pub deleted: Vec<String>,
    /// Next cursor token to pass on subsequent sync calls.
    pub next_cursor: Option<String>,
    /// Whether more changes remain to be fetched.
    pub has_more: bool,
    /// True when the request carried a cursor and nothing has changed since.
    /// Distinguishes "no new rows" from "same cursor echoed back", which the
    /// client cannot otherwise tell apart.
    pub unchanged: bool,
}

/// Response payload containing change deltas across requested resources.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncChangesResponse {
    /// Current server timestamp in UTC.
    pub server_time: DateTime<Utc>,
    /// Map of resource name (e.g. "products") to its change payload.
    pub changes: HashMap<String, serde_json::Value>,
}

/// Per-resource watermark. `cursor` is the opaque cursor for the newest row
/// in that collection, so a client that has just taken a full snapshot can
/// adopt it directly and have its next pull be a true delta — deriving one
/// from `GET /sync/changes` is not possible, since that endpoint pages
/// oldest-first and would hand back the *first* row's cursor.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSyncStatus {
    /// Timestamp of most recent record update in this collection.
    pub last_updated_at: DateTime<Utc>,
    /// Opaque cursor token for newest record in this collection.
    pub cursor: String,
}

/// Response payload containing the latest sync watermark for all resources.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusResponse {
    /// Current server timestamp in UTC.
    pub server_time: DateTime<Utc>,
    /// Map of resource name to current sync watermark status.
    pub resources: HashMap<String, ResourceSyncStatus>,
}
