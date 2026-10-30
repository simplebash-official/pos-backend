use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeedResponse {
    /// Rows enqueued across all synced tables.
    pub enqueued: i64,
}
