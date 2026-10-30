use crate::modules::tenants::usage::TenantUsage;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUsageResponse {
    pub items: Vec<TenantUsage>,
}
