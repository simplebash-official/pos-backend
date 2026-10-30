use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `DELETE /products` (batch delete). Any id that fails to parse
/// as an `ObjectId` is silently dropped rather than rejecting the whole
/// request — see `service::product::delete_products`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsRequest {
    /// List of product IDs or keys to delete.
    pub product_ids: Vec<String>,
}
