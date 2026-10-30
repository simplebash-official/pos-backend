use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for Daily Sales report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DailySalesQuery {
    /// Target date (YYYY-MM-DD), defaults to today.
    pub date: Option<String>,
    /// Optional start date (YYYY-MM-DD) for multi-day daily report.
    pub from: Option<String>,
    /// Optional end date (YYYY-MM-DD) for multi-day daily report.
    pub to: Option<String>,
}
