use serde::Serialize;
use utoipa::ToSchema;

/// One month's cost.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthTotal {
    /// `YYYY-MM`.
    pub month: String,
    pub total_cents: i64,
}
