use super::print_job::PrintJob;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response payload containing a list of print jobs.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrintJobListResponse {
    /// List of print jobs on current page.
    pub print_jobs: Vec<PrintJob>,
    /// Total count of matching print jobs.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Items limit per page.
    pub limit: u64,
    /// Total number of pages.
    pub total_pages: u64,
}
