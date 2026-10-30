use super::import_batch::ImportBatch;
use crate::domain::inventory::PaginationMeta;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response for `GET /api/imports`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchListResponse {
    /// List of import batches on the current page.
    pub items: Vec<ImportBatch>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}
