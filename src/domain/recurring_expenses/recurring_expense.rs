use crate::domain::expenses::PaidFrom;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A regular bill the shop pays every month.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecurringExpense {
    pub id: String,
    pub key: String,
    pub category_key: String,
    /// Live name of the category, so a rename shows here.
    pub category_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    /// What is usually paid each month.
    pub amount_cents: i64,
    /// Day of the month it is due, 1 to 31; a month with fewer days uses its last day.
    pub due_day: i64,
    /// How many days before the due day it starts showing as a reminder.
    pub remind_days_before: i64,
    /// Where it is usually paid from: `drawer`, `bank`, `card` or `online`.
    pub paid_from: PaidFrom,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Stopped templates stay on the record but no longer remind.
    pub active: bool,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
