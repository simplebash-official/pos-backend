// Paginated response of login sessions for a shop.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::shop_login_item::ShopLoginItem;

/// Paginated login sessions result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopLoginsResponse {
    pub items: Vec<ShopLoginItem>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}
