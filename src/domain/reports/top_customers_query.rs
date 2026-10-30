use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

// --- top customers -------------------------------------------------------

/// Query for `GET /reports/analytics/top-customers`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TopCustomersQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Max rows, 1–100 (default 20).
    pub limit: Option<u64>,
    /// `revenue` (default) | `invoices` | `profit`.
    pub sort_by: Option<String>,
}
