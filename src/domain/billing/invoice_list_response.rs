use super::invoice::Invoice;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response payload containing invoice list.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceListResponse {
    /// List of invoices for the requested page.
    pub invoices: Vec<Invoice>,
    /// Total count of matching invoices.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Maximum items per page.
    pub limit: u64,
    /// Total number of pages available.
    pub total_pages: u64,
}
