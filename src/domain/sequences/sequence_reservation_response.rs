use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response payload containing details of reserved sequence range.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SequenceReservationResponse {
    /// Name of the sequence entity (e.g. "invoice", "repair").
    pub name: String,
    /// Identifier key of the reserved block.
    pub block_id: String,
    /// Prefix attached to sequence numbers (e.g. "INV-").
    pub prefix: String,
    /// Zero-padding digit length.
    pub padding: usize,
    /// First number available in reserved range.
    pub start: u64,
    /// Last number available in reserved range.
    pub end: u64,
    /// Expiry date/time for this sequence block reservation.
    pub expires_at: DateTime<Utc>,
}
