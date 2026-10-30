use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NumberBlockInfo {
    pub name: String,
    /// Numbers left across the non-expired blocks of this name.
    pub remaining: i64,
    /// Size of the newest block, so the agent can refill at 20 %.
    pub block_size: i64,
}
