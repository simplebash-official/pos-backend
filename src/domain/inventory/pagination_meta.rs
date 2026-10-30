use serde::Serialize;
use utoipa::ToSchema;

/// Pagination metadata attached to any paginated list response.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    /// Current page number.
    pub page: u64,
    /// Items per page limit.
    pub limit: u64,
    /// Total records across all pages.
    pub total: u64,
    /// Total number of pages.
    pub total_pages: u64,
}
