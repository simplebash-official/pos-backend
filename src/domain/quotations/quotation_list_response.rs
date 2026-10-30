use super::quotation::Quotation;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated list payload for `GET /quotations`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotationListResponse {
    pub quotations: Vec<Quotation>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
