use super::ack_status::AckStatus;

use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

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
