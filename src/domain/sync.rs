use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::{IntoParams, ToSchema};

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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceChanges<T: Serialize> {
    pub full: bool,
    pub items: Vec<T>,
    pub deleted: Vec<String>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    /// True when the request carried a cursor and nothing has changed since.
    /// Distinguishes "no new rows" from "same cursor echoed back", which the
    /// client cannot otherwise tell apart.
    pub unchanged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncChangesResponse {
    pub server_time: DateTime<Utc>,
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
    pub last_updated_at: DateTime<Utc>,
    pub cursor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusResponse {
    pub server_time: DateTime<Utc>,
    pub resources: HashMap<String, ResourceSyncStatus>,
}
