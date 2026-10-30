use super::change_record::ChangeRecord;

use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulledChange {
    pub seq: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_device_id: Option<String>,
    #[serde(flatten)]
    pub record: ChangeRecord,
}
