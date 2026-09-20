// Wire and hand-off types of sync v2 (two-way device <-> cloud sync). Pure data:
// no I/O, no framework types beyond serde/utoipa derives. The wire shape is
// pinned by the sync v2 spec; PKG-2 (SQLite device), PKG-3 (Mongo cloud) and
// PKG-4 (desktop agent) all compile against these definitions, so change them
// only together.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Whether a change writes the row or tombstones it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ChangeOp {
    Upsert,
    Delete,
}

/// One canonical change, identical on push, pull and snapshot. `payload` is the
/// hydrated REST DTO plus the legacy `id`; delete records carry none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRecord {
    /// Wire resource name, e.g. `stockMovements`.
    pub resource: String,
    pub key: String,
    pub op: ChangeOp,
    pub version: i64,
    pub updated_at: DateTime<Utc>,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PushRequest {
    pub device_id: String,
    pub batch_id: String,
    pub base_seq: i64,
    pub changes: Vec<ChangeRecord>,
    /// Outbox sequence number of each change, parallel to `changes`.
    #[serde(default)]
    pub outbox_seqs: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PushResponse {
    pub server_time: DateTime<Utc>,
    pub acks: Vec<ChangeAck>,
    pub server_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeAck {
    pub key: String,
    pub resource: String,
    pub status: AckStatus,
    /// Stable reason string: IMMUTABLE_MISMATCH, UNKNOWN_RESOURCE,
    /// INVALID_PAYLOAD, CLOCK_CLAMPED, LWW_LOSER, SERIAL_DOUBLE_SOLD,
    /// DEVICE_REVOKED.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// True when the server clamped a future `updatedAt` (status stays applied).
    #[serde(default)]
    pub clamped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum AckStatus {
    Applied,
    Duplicate,
    Conflict,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    pub server_time: DateTime<Utc>,
    pub changes: Vec<PulledChange>,
    pub next_seq: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulledChange {
    pub seq: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_device_id: Option<String>,
    #[serde(flatten)]
    pub record: ChangeRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotResponse {
    pub as_of_seq: i64,
    pub changes: Vec<ChangeRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ApplyMode {
    Incremental,
    Bootstrap,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub mode: ApplyMode,
    pub changes: Vec<PulledChange>,
    #[serde(default)]
    pub advance_cursor_to: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResponse {
    pub applied: u32,
    pub duplicates: u32,
    pub conflicts: u32,
    pub cursor: i64,
}

/// Kinds of automatically resolved but reviewable conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictKind {
    LwwLoser,
    SerialDoubleSold,
    OverRefund,
    NegativeStock,
    UniqueViolation,
}

impl ConflictKind {
    /// Stable string stored in `sync_conflicts.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            ConflictKind::LwwLoser => "LWW_LOSER",
            ConflictKind::SerialDoubleSold => "SERIAL_DOUBLE_SOLD",
            ConflictKind::OverRefund => "OVER_REFUND",
            ConflictKind::NegativeStock => "NEGATIVE_STOCK",
            ConflictKind::UniqueViolation => "UNIQUE_VIOLATION",
        }
    }
}

/// A conflict raised while applying changes or recomputing derived fields;
/// persisted to `sync_conflicts` by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct NewConflict {
    pub kind: ConflictKind,
    pub resource: String,
    pub entity_key: String,
    pub detail: serde_json::Value,
}

/// Entities whose derived fields must be recomputed after a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DerivedScope {
    pub product_ids: BTreeSet<String>,
    pub customer_keys: BTreeSet<String>,
    pub invoice_keys: BTreeSet<String>,
}

impl DerivedScope {
    pub fn is_empty(&self) -> bool {
        self.product_ids.is_empty() && self.customer_keys.is_empty() && self.invoice_keys.is_empty()
    }

    pub fn merge(&mut self, other: DerivedScope) {
        self.product_ids.extend(other.product_ids);
        self.customer_keys.extend(other.customer_keys);
        self.invoice_keys.extend(other.invoice_keys);
    }
}

/// Who a batch of changes came from; stamps `updated_by_device` / row meta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOrigin {
    Device(String),
    Cloud,
}

/// Result of applying changes for one resource.
#[derive(Debug, Clone, Default)]
pub struct ApplyOutcome {
    pub applied: u32,
    pub duplicates: u32,
    pub conflicts: Vec<NewConflict>,
    pub touched: DerivedScope,
}
