use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PUT /balances/{kind}/{key}/opening`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetBalanceOpeningRequest {
    /// What was owed on `asOfDate` (for a supplier, what the shop owed; for staff, what they owed the shop).
    pub opening_balance_cents: i64,
    /// `YYYY-MM-DD`, today or earlier.
    pub as_of_date: String,
}
