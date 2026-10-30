// Query parameters for filtering and paginating a shop's login history.

use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for GET /tenants/{tenant_id}/logins.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ShopLoginsQuery {
    pub user_key: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
