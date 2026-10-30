use crate::domain::expenses::PaidFrom;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /recurring-expenses`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecurringExpenseRequest {
    pub category_key: String,
    #[serde(default)]
    pub payee_key: Option<String>,
    pub amount_cents: i64,
    pub due_day: i64,
    pub remind_days_before: i64,
    pub paid_from: PaidFrom,
    #[serde(default)]
    pub note: Option<String>,
}
