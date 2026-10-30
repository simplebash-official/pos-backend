use super::lookup_shop_code::lookup_shop_code;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::core::tenancy::Tenant;

/// Resolves a shop code to its tenant scope; 404 `TENANT_NOT_FOUND` if unknown.
pub async fn resolve_shop_code(db: &Db, code: &str) -> AppResult<Tenant> {
    lookup_shop_code(db, code).await?.ok_or_else(|| {
        AppError::not_found_with_code(
            format!("No shop with code '{}'", code.trim()),
            codes::TENANT_NOT_FOUND,
        )
    })
}
