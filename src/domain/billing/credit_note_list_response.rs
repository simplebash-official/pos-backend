use super::credit_note::CreditNote;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response payload containing list of credit notes.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreditNoteListResponse {
    /// List of credit notes for the requested page.
    pub credit_notes: Vec<CreditNote>,
    /// Total count of matching credit notes.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Maximum items per page.
    pub limit: u64,
    /// Total number of pages available.
    pub total_pages: u64,
}
