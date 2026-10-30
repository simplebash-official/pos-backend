use serde::Deserialize;

use utoipa::IntoParams;

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
