use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

// --- sales by category ---------------------------------------------------

/// Query for `GET /reports/analytics/sales-by-category`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SalesByCategoryQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// `category` (default) | `subcategory`.
    pub group_by: Option<String>,
}
