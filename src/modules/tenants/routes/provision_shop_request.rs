use super::provision_owner_request::ProvisionOwnerRequest;

use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionShopRequest {
    /// The identity service's tenant id (`tnt_...`).
    pub tenant_id: String,
    pub shop_code: String,
    pub name: String,
    /// Present for web sign-ups; absent for desktop ones (their admin arrives via device sync).
    pub owner: Option<ProvisionOwnerRequest>,
}
