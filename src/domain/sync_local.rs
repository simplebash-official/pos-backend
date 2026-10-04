// Request/response types of the device-local sync API (`/api/sync/state`,
// `/outbox`, `/apply`, `/blocks`, `/conflicts`). Only the desktop shell's sync
// agent calls these, with a service token. Pure data, camelCase on the wire.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::sync_v2::ChangeRecord;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncStateResponse {
    pub device_id: String,
    pub tenant_id: Option<String>,
    pub linked: bool,
    pub capture_enabled: bool,
    pub cloud_cursor: i64,
    pub last_pushed_outbox_seq: i64,
    pub clock_offset_ms: i64,
    pub bootstrap_active: bool,
    /// Outbox rows waiting to be pushed.
    pub pending_out: i64,
    /// Unresolved conflicts.
    pub conflicts_open: i64,
    /// True when the device already holds sales (a first-device upload, or a
    /// join that must not silently overwrite them).
    pub local_has_data: bool,
    /// Cloud-reserved number blocks still usable, per sequence name.
    pub number_blocks: Vec<NumberBlockInfo>,
    /// Outbox rows waiting to be pushed, per resource (wire name).
    pub pending_by_resource: Vec<ResourceCount>,
    /// Unresolved conflicts, per resource (wire name).
    pub conflicts_by_resource: Vec<ResourceCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCount {
    pub resource: String,
    pub count: i64,
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PendingQuery {
    /// Only this resource (wire name, e.g. `invoices`).
    pub resource: Option<String>,
    /// Page size, 1..200 (default 50).
    pub limit: Option<i64>,
}

/// One change that has not reached the cloud yet, described for people.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingItem {
    pub resource: String,
    pub key: String,
    /// `upsert` or `delete`.
    pub op: String,
    pub enqueued_at: String,
    /// Invoice number, product name, etc.; `None` when the row is gone or has no readable name.
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingResponse {
    pub items: Vec<PendingItem>,
    /// Total waiting for the requested resource(s), which can exceed `items`.
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NumberBlockInfo {
    pub name: String,
    /// Numbers left across the non-expired blocks of this name.
    pub remaining: i64,
    /// Size of the newest block, so the agent can refill at 20 %.
    pub block_size: i64,
}

/// Response of `GET /api/sync/sku-prefixes`: the distinct SKU block families
/// (derived category+subcategory codes, e.g. "PHO-SCR") this device's local
/// catalog currently needs a cloud-reserved number block for.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkuPrefixesResponse {
    pub prefixes: Vec<String>,
}

/// Body of `POST /api/sync/enable`: the cloud-assigned identity of this device.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnableRequest {
    pub tenant_id: Option<String>,
    /// Replaces the locally generated device id with the one the cloud issued.
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSyncStateRequest {
    pub clock_offset_ms: Option<i64>,
    pub linked: Option<bool>,
    pub tenant_id: Option<String>,
    pub cloud_cursor: Option<i64>,
    /// Clears an interrupted bootstrap so the next bootstrap page wipes again.
    pub bootstrap_reset: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct OutboxQuery {
    /// Return outbox rows with a sequence number greater than this (default 0).
    pub after: Option<i64>,
    /// Page size, 1..500 (default 200).
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxItem {
    pub seq: i64,
    pub record: ChangeRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxResponse {
    pub items: Vec<OutboxItem>,
    /// Highest outbox sequence number covered by `items` (or `after` when empty).
    pub last_seq: i64,
    /// Identity of this database's outbox numbering; part of every upload
    /// batch id so ids never repeat across a recreated database.
    pub epoch: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxAckRequest {
    pub up_to_seq: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeedResponse {
    /// Rows enqueued across all synced tables.
    pub enqueued: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnableResponse {
    pub device_id: String,
    /// Opening-balance stock movements created to make stock a pure ledger.
    pub opening_balances_created: i64,
    pub enqueued: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreBlockRequest {
    /// Sequence name, e.g. `invoice`.
    pub name: String,
    pub prefix: String,
    pub padding: i64,
    pub start: i64,
    pub end: i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConflictItem {
    pub key: String,
    pub kind: String,
    pub resource: String,
    pub entity_key: String,
    pub detail: serde_json::Value,
    pub detected_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveConflictRequest {
    pub resolution: String,
}
