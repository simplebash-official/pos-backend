use super::customer::Customer;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /customers` matching frontend paginated structure.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerListResponse {
    /// List of customer records on the current page.
    pub customers: Vec<Customer>,
    /// Total count of matching customer records.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Number of items per page.
    pub limit: u64,
    /// Total number of pages available.
    pub total_pages: u64,
}
