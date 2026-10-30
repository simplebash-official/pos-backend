use super::pagination_meta::PaginationMeta;
use super::product::Product;

use serde::Serialize;
use utoipa::ToSchema;

/// Response body for `GET /products`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductListResponse {
    /// List of products for the requested page.
    pub items: Vec<Product>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}
