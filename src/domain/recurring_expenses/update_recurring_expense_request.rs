use crate::domain::expenses::PaidFrom;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /recurring-expenses/{key}`. Absent fields are left unchanged; the category and
/// payee are what identify a regular bill, so they cannot be changed (stop it and add a new one).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecurringExpenseRequest {
    #[serde(default)]
    pub amount_cents: Option<i64>,
    #[serde(default)]
    pub due_day: Option<i64>,
    #[serde(default)]
    pub remind_days_before: Option<i64>,
    #[serde(default)]
    pub paid_from: Option<PaidFrom>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub active: Option<bool>,
}
