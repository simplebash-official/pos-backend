use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// Part of an advance taken back from pay.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdvanceRecovery {
    pub key: String,
    pub employee_key: String,
    pub amount_cents: i64,
    /// `YYYY-MM-DD`.
    pub recovered_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}
