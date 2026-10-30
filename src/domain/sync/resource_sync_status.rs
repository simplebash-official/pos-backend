use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

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
