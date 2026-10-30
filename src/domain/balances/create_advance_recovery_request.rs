use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /balances/staff/{employeeKey}/recoveries`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateAdvanceRecoveryRequest {
    pub amount_cents: i64,
    /// `YYYY-MM-DD`.
    pub recovered_on: String,
    #[serde(default)]
    pub note: Option<String>,
}
