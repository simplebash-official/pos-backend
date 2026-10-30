use super::received_into::ReceivedInto;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /incomes`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateIncomeRequest {
    /// `YYYY-MM-DD`.
    pub income_date: String,
    pub category_key: String,
    #[serde(default)]
    pub payee_key: Option<String>,
    #[serde(default)]
    pub payee_name: Option<String>,
    pub amount_cents: i64,
    /// `bank`, `card` or `online`; `drawer` is refused because that is a cash movement.
    pub received_into: ReceivedInto,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}
