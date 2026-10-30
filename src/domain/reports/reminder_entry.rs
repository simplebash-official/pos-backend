use super::reminder_kind::ReminderKind;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One actionable reminder for the dashboard's attention list.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReminderEntry {
    pub kind: ReminderKind,
    /// Mongo hex id of the underlying invoice / job.
    pub id: String,
    /// Prefixed key (`inv_…` / `rep_…` / `prj_…`) of the underlying record.
    pub key: String,
    /// Human-facing number: invoice number or ticket number.
    pub reference_number: String,
    /// Short label — customer name for a credit reminder, device/job
    /// description for a job reminder.
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// Credit: the balance still owed. Job: the estimated cost (0 if unquoted).
    pub amount_cents: i64,
    /// The due / promised date (YYYY-MM-DD) — always present.
    pub due_date: String,
    /// Signed day delta: positive = overdue by N days, negative = due in N
    /// days, 0 = due today.
    pub days_from_due: i64,
    pub severity: String,
    /// Frontend deep link that opens the record with the right action.
    pub link_to: String,
}
