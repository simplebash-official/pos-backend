use serde::Serialize;
use utoipa::ToSchema;

/// Response for `DELETE /customers/batch`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCustomersResponse {
    /// Number of customer records deleted.
    pub deleted_count: u64,
}
