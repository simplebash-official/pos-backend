use super::repair::Repair;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response payload containing list of repair tickets.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepairListResponse {
    /// List of repair tickets on current page.
    pub repairs: Vec<Repair>,
    /// Total count of matching repair tickets.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Items limit per page.
    pub limit: u64,
    /// Total number of pages.
    pub total_pages: u64,
}
