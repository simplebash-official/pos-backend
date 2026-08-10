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
    /// Limit per resource type (1..500, default 500)
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
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncChangesResponse {
    pub server_time: DateTime<Utc>,
    pub changes: HashMap<String, serde_json::Value>,
}
