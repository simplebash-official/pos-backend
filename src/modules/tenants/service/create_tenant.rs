use super::create_tenant_with_key::create_tenant_with_key;
use super::tenant_info::TenantInfo;
use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;

/// Registers a tenant. 409 `SHOP_CODE_ALREADY_EXISTS` if the code is taken.
pub async fn create_tenant(db: &Db, shop_code: &str, name: &str) -> AppResult<TenantInfo> {
    create_tenant_with_key(db, generate_id(prefixes::TENANT), shop_code, name).await
}
