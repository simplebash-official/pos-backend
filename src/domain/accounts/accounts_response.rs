use super::account_summary::AccountSummary;
use serde::Serialize;
use utoipa::ToSchema;

/// Every account, drawer first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResponse {
    pub accounts: Vec<AccountSummary>,
}
