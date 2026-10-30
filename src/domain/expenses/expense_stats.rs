use serde::Serialize;
use utoipa::ToSchema;

/// Headline figures for the Expenses page.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseStats {
    /// Everything spent this month (UTC), whatever it was paid from.
    pub this_month_cents: i64,
    /// Of that, paid from the drawer.
    pub this_month_drawer_cents: i64,
    /// Still owed on bills, all months.
    pub unpaid_cents: i64,
    pub unpaid_count: u64,
    /// Of the unpaid, past their due date.
    pub overdue_cents: i64,
    pub overdue_count: u64,
}
