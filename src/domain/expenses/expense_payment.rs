use super::paid_from::PaidFrom;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One payment against an expense.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpensePayment {
    pub id: String,
    pub key: String,
    pub expense_key: String,
    pub amount_cents: i64,
    pub paid_from: PaidFrom,
    /// The day the money left (`YYYY-MM-DD`).
    pub paid_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub recorded_by_name_snapshot: String,
    pub created_at: DateTime<Utc>,
}
