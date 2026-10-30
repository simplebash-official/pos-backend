// Query parameters for filtering and paginating a shop's business activities.

use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for GET /tenants/{tenant_id}/activities.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ShopActivitiesQuery {
    pub user_key: Option<String>,
    pub kind: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
