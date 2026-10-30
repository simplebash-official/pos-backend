use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /auth/login`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    /// User's login username (unique within the shop).
    pub username: String,
    /// User's plain text password.
    pub password: String,
    /// Shop (tenant) the account belongs to. Required on multi-tenant
    /// deployments (`TENANT_MODE=multi`), ignored on single-shop ones.
    #[serde(default, alias = "shop_code")]
    pub shop_code: Option<String>,
}
