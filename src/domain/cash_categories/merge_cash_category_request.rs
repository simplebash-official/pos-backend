use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /cash-categories/{key}/merge`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MergeCashCategoryRequest {
    pub target_key: String,
}
