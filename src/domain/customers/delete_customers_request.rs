use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `DELETE /customers/batch`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeleteCustomersRequest {
    /// List of customer IDs or keys to delete.
    pub ids: Vec<String>,
}
