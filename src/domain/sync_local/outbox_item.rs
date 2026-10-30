use super::super::sync_v2::ChangeRecord;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutboxItem {
    pub seq: i64,
    pub record: ChangeRecord,
}
