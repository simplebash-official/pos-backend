use super::supplier_credit::SupplierCredit;
use crate::domain::inventory::PaginationMeta;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response for `GET /supplier-credits`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierCreditListResponse {
    pub items: Vec<SupplierCredit>,
    pub pagination: PaginationMeta,
}
