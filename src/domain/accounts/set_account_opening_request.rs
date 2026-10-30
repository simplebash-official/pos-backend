use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PUT /accounts/{kind}/opening`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetAccountOpeningRequest {
    /// What the account held on `asOfDate`; may be negative for an overdraft.
    pub opening_balance_cents: i64,
    /// `YYYY-MM-DD`, today or earlier.
    pub as_of_date: String,
}
