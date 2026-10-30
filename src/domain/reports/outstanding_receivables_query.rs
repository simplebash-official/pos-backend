use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for Outstanding Receivables report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingReceivablesQuery {
    /// Optional customer name, phone, or invoice number search filter.
    pub search: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
