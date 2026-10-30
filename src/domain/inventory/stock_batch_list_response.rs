use super::stock_batch::StockBatch;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /products/{key}/batches`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StockBatchListResponse {
    /// Every batch of the product, newest first (empty ones included, so the
    /// intake history stays visible).
    pub items: Vec<StockBatch>,
}
