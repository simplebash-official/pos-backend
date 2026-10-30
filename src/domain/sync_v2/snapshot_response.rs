use super::change_record::ChangeRecord;

use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotResponse {
    pub as_of_seq: i64,
    pub changes: Vec<ChangeRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}
