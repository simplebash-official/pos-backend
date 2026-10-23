// Pure business types for the shop's money accounts: the cash drawer, the bank, cards and online.
// No I/O, no framework types beyond serde/utoipa derives.
//
// A balance is never stored. The drawer is the running cash of its terminals; the bank, card and
// online accounts start from an opening balance the owner sets once and are worked out from what
// moved on and after that day.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One of the shop's money accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Drawer,
    Bank,
    Card,
    Online,
}

impl AccountKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountKind::Drawer => "drawer",
            AccountKind::Bank => "bank",
            AccountKind::Card => "card",
            AccountKind::Online => "online",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "drawer" => Some(AccountKind::Drawer),
            "bank" => Some(AccountKind::Bank),
            "card" => Some(AccountKind::Card),
            "online" => Some(AccountKind::Online),
            _ => None,
        }
    }
}

/// What moved through an account since its opening day. A line that cannot happen to this kind of
/// account (card sales into the bank) is zero.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountActivity {
    /// Customer payments received by card or online.
    pub sales_cents: i64,
    /// Customer refunds paid back by card or online.
    pub refunds_cents: i64,
    /// Other income received here (not a sale).
    pub income_cents: i64,
    /// Costs paid from here.
    pub expenses_cents: i64,
    /// Cash taken from the drawer to the bank.
    pub deposits_cents: i64,
    /// Cash brought from the bank to the drawer.
    pub withdrawals_cents: i64,
}

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

/// Every account, drawer first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResponse {
    pub accounts: Vec<AccountSummary>,
}

/// Body for `PUT /accounts/{kind}/opening`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetAccountOpeningRequest {
    /// What the account held on `asOfDate`; may be negative for an overdraft.
    pub opening_balance_cents: i64,
    /// `YYYY-MM-DD`, today or earlier.
    pub as_of_date: String,
}
