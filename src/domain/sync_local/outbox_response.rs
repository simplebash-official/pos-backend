use super::outbox_item::OutboxItem;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
