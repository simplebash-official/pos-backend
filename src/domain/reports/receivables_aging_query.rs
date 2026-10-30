use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

// --- receivables aging -------------------------------------------------

/// Query for `GET /reports/analytics/receivables-aging`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ReceivablesAgingQuery {
    /// Snapshot date (YYYY-MM-DD), defaults to today.
    pub as_of: Option<String>,
}
