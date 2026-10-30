use serde::Serialize;
use utoipa::ToSchema;

/// Headline figures for the Income page.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStats {
    /// Everything received this month (UTC), wherever it went.
    pub this_month_cents: i64,
    /// Of that, put into the drawer.
    pub this_month_drawer_cents: i64,
    pub last_month_cents: i64,
}
