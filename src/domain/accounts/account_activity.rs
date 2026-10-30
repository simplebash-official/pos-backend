use serde::Serialize;
use utoipa::ToSchema;

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
