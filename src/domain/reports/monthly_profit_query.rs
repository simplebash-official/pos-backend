use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for Monthly Profit report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyProfitQuery {
    /// Month in YYYY-MM format (e.g. "2026-08"), defaults to current month.
    pub month: Option<String>,
    /// Year in YYYY format (e.g. "2026"), defaults to current year.
    pub year: Option<i32>,
}
