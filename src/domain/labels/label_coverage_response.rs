use super::label_coverage_counts::LabelCoverageCounts;
use super::label_coverage_row::LabelCoverageRow;
use crate::domain::inventory::PaginationMeta;

use serde::Serialize;
use utoipa::ToSchema;

/// `GET /labels/coverage` result: never labelled first, then labels needed,
/// then up to date, each by name.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelCoverageResponse {
    pub items: Vec<LabelCoverageRow>,
    pub pagination: PaginationMeta,
    pub counts: LabelCoverageCounts,
}
