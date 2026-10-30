use super::change_record::ChangeRecord;

use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

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
