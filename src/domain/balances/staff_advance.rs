use serde::Serialize;
use utoipa::ToSchema;

/// What one staff member still owes on advances and staff loans.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffAdvance {
    pub employee_key: String,
    pub name: String,
    /// Owed before the records began, when the owner entered it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_balance_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_date: Option<String>,
    /// Advances and staff loans handed out (counted from the opening day when there is one).
    pub given_cents: i64,
    pub recovered_cents: i64,
    pub outstanding_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_given_on: Option<String>,
}
