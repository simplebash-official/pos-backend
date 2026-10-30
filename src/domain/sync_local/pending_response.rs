use super::pending_item::PendingItem;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingResponse {
    pub items: Vec<PendingItem>,
    /// Total waiting for the requested resource(s), which can exceed `items`.
    pub total: i64,
}
