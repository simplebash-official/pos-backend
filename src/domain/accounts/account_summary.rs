use super::account_activity::AccountActivity;
use super::account_kind::AccountKind;
use serde::Serialize;
use utoipa::ToSchema;

/// One account with its balance. The bank, card and online accounts have no balance until an opening balance is set.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub kind: AccountKind,
    /// Where the account started; absent for the drawer and for an account not set up yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opening_balance_cents: Option<i64>,
    /// `YYYY-MM-DD`: activity from this day on counts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of_date: Option<String>,
    /// What the account holds now; absent until an opening balance is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_cents: Option<i64>,
    /// Absent for the drawer and for an account not set up yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<AccountActivity>,
}
