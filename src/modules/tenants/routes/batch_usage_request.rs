use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUsageRequest {
    /// Up to 100 tenant ids; ids that are not in the directory are left out of the answer.
    pub tenant_ids: Vec<String>,
}
