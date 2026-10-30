use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for the dashboard reminders feed (`GET /reports/reminders`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct RemindersQuery {
    /// How many days ahead counts as "due soon" (not yet overdue). Clamped
    /// to 1..=30; defaults to 7.
    pub due_within_days: Option<u32>,
    /// Max reminders returned in the merged list (headline counts still
    /// reflect the full set). Clamped to 1..=100; defaults to 20.
    pub limit: Option<u64>,
    /// Page number (1-indexed). Defaults to 1.
    pub page: Option<u64>,
}
