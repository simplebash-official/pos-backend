use super::number_block_info::NumberBlockInfo;
use super::resource_count::ResourceCount;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
