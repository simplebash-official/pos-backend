use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionShopResponse {
    /// False when the shop was already registered (a replay).
    pub tenant_created: bool,
    /// False when the shop already had an Admin, or no owner was supplied.
    pub admin_created: bool,
}
