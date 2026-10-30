use super::label_job::LabelJob;
use crate::domain::inventory::PaginationMeta;

use serde::Serialize;
use utoipa::ToSchema;

/// `GET /labels/jobs` result, newest first.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelJobListResponse {
    pub items: Vec<LabelJob>,
    pub pagination: PaginationMeta,
}
