use serde::Serialize;
use utoipa::ToSchema;

/// Response for the batch-delete endpoint — count only, since the caller
/// already knows which ids it asked to delete.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProductsResponse {
    /// Number of products successfully deleted.
    pub deleted_count: u64,
}
