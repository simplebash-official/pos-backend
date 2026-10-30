use super::label_coverage_status::LabelCoverageStatus;

use serde::Deserialize;
use utoipa::IntoParams;

/// Query of `GET /labels/coverage`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct LabelCoverageQuery {
    pub page: Option<u64>,
    pub limit: Option<u64>,
    /// Only products in this state; omitted lists every product.
    pub status: Option<LabelCoverageStatus>,
    /// Matches name, SKU, barcode or category, like the inventory list.
    pub search: Option<String>,
    /// Comma-separated product keys (at most 200): only these products, e.g.
    /// to print a past job again with today's details. Ignores `search`.
    pub product_keys: Option<String>,
}
