use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for Top Products report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TopProductsQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<u64>,
    /// Sort criteria: `"revenue"` or `"quantity"`, defaults to `"revenue"`.
    pub sort_by: Option<String>,
}
