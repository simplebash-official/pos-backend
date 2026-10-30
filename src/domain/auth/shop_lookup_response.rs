use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /auth/shop/{code}`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopLookupResponse {
    /// Unique shop code.
    pub shop_code: String,
    /// Display name of the shop.
    pub name: String,
}
