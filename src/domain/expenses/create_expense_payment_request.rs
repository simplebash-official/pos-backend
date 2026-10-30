use super::paid_from::PaidFrom;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /expenses/{key}/payments`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateExpensePaymentRequest {
    pub amount_cents: i64,
    /// `bank`, `card` or `online`.
    pub paid_from: PaidFrom,
    /// `YYYY-MM-DD`.
    pub paid_on: String,
    #[serde(default)]
    pub reference_no: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}
