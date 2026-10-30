use super::change_op::ChangeOp;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

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
