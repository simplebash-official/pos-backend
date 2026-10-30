use serde::Serialize;
use utoipa::ToSchema;

/// How many products are in each state (ignores `status`, honours `search`).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelCoverageCounts {
    pub never: u64,
    pub needed: u64,
    pub labelled: u64,
}
