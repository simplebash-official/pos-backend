use super::product_serial::ProductSerial;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /products/{key}/serials`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSerialListResponse {
    /// Matching serial units for the requested product.
    pub items: Vec<ProductSerial>,
}
