use serde::Serialize;
use utoipa::ToSchema;

/// A bill that is not paid in full.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PayableBill {
    pub key: String,
    pub number: String,
    pub category_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    pub balance_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub is_overdue: bool,
}
