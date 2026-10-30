use super::purchase_receipt::PurchaseReceipt;
use crate::domain::inventory::PaginationMeta;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated response for `GET /purchase-receipts`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptListResponse {
    pub items: Vec<PurchaseReceipt>,
    pub pagination: PaginationMeta,
}
