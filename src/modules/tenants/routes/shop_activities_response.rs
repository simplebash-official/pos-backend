// Paginated response of business activities for a shop.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::shop_activity_item::ShopActivityItem;

/// Paginated business activities result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopActivitiesResponse {
    pub items: Vec<ShopActivityItem>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}
