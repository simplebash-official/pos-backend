use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::core::tenancy::{Tenant, with_tenant};

use crate::modules::tenants::repository;

/// Runs `work` inside the shop's tenant scope; 404 `TENANT_NOT_FOUND` for a
/// shop that is not in the directory.
pub(super) async fn in_shop<T, F: std::future::Future<Output = AppResult<T>>>(
    db: &Db,
    tenant_id: &str,
    work: F,
) -> AppResult<T> {
    repository::find_tenant_by_key(db, tenant_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code(
                format!("No shop with id '{tenant_id}'"),
                codes::TENANT_NOT_FOUND,
            )
        })?;
    with_tenant(Tenant::id(tenant_id)?, work).await
}
