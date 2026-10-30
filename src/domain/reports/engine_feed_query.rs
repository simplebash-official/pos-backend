use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for the unified Analytics & Chart Engine feed endpoint.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct EngineFeedQuery {
    /// Section requested: `"overview"`, `"sales"`, `"profit"`, `"customers"`, `"staff"`, or `"all"`. Defaults to `"overview"`.
    pub section: Option<String>,
    /// Date preset: `"today"`, `"yesterday"`, `"this_week"`, `"this_month"`, `"last_month"`, `"this_year"`, or `"custom"`.
    pub preset: Option<String>,
    /// Start date (YYYY-MM-DD or RFC3339 timestamp) for custom ranges.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD or RFC3339 timestamp) for custom ranges.
    pub to: Option<String>,
    /// Granularity: `"day"`, `"week"`, `"month"`, or `"year"`.
    pub granularity: Option<String>,
    /// When true, compute previous period comparison and deltas.
    pub compare_previous: Option<bool>,
    /// Limit for ranked lists (top products, customers). Defaults to 20.
    pub limit: Option<u64>,
    /// Sort criteria for top customers/products (`"revenue"`, `"invoices"`, `"profit"`, `"quantity"`).
    pub sort_by: Option<String>,
    /// Grouping for category sales (`"category"` or `"subcategory"`).
    pub group_by: Option<String>,
    /// Force bypass cache (admin debugging).
    pub bypass_cache: Option<bool>,
}
