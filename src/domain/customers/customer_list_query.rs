use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /customers`. `search` matches against name,
/// phone numbers, contact person, email, and address; `tag` filters by
/// membership in `tags`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CustomerListQuery {
    /// Keyword search query.
    pub search: Option<String>,
    /// Tag filter (exact match within tags array).
    pub tag: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Maximum items per page.
    pub limit: Option<u64>,
    /// Field name to sort by.
    pub sort_by: Option<String>,
    /// Sort direction ("asc" or "desc").
    pub sort_order: Option<String>,
}
