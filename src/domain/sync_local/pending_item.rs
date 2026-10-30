use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One change that has not reached the cloud yet, described for people.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingItem {
    pub resource: String,
    pub key: String,
    /// `upsert` or `delete`.
    pub op: String,
    pub enqueued_at: String,
    /// Invoice number, product name, etc.; `None` when the row is gone or has no readable name.
    pub label: Option<String>,
}
