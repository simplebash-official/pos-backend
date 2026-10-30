use super::reminder_entry::ReminderEntry;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

fn default_one() -> u64 {
    1
}

fn default_twenty() -> u64 {
    20
}

/// The dashboard reminders feed: a merged, sorted list plus honest headline
/// counts over the full (pre-truncation) set.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemindersResponse {
    /// Overdue reminders first, then due-soon; each group ordered by date.
    pub reminders: Vec<ReminderEntry>,
    pub credit_overdue_count: u64,
    pub credit_overdue_amount_cents: i64,
    pub credit_due_soon_count: u64,
    pub credit_due_soon_amount_cents: i64,
    pub jobs_overdue_count: u64,
    pub jobs_due_soon_count: u64,
    pub due_within_days: u32,
    #[serde(default)]
    pub total: u64,
    #[serde(default = "default_one")]
    pub page: u64,
    #[serde(default = "default_twenty")]
    pub limit: u64,
    #[serde(default = "default_one")]
    pub total_pages: u64,
}
