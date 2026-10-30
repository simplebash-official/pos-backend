use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `DELETE /suppliers/batch`. Ids that fail to delete (invalid
/// `ObjectId`, or blocked by the `SUPPLIER_HAS_PURCHASES` guard) are
/// silently skipped rather than failing the whole request — see
/// `service::delete_suppliers`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeleteSuppliersRequest {
    /// List of supplier Mongo hex IDs to delete.
    pub ids: Vec<String>,
}
