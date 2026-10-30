use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for listing past import batches.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ImportBatchListQuery {
    /// Filter batches by target module (e.g. "inventory").
    pub target: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}
