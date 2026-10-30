use super::pulled_change::PulledChange;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    pub server_time: DateTime<Utc>,
    pub changes: Vec<PulledChange>,
    pub next_seq: i64,
    pub has_more: bool,
}
