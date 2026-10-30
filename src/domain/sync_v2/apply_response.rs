use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResponse {
    pub applied: u32,
    pub duplicates: u32,
    pub conflicts: u32,
    pub cursor: i64,
}
