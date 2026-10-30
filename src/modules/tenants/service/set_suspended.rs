use super::suspension_cache::suspension_cache;
use super::validate_tenant_key::validate_tenant_key;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::tenants::repository;

use mongodb::bson::DateTime as BsonDateTime;

/// Switches a shop's cloud access off (`Some((since, reason))`, identity's own time and reason) or
/// on (`None`). Called by the identity service when an operator suspends or reinstates the owner's
/// account. Idempotent. 404 `TENANT_NOT_FOUND` for an unknown shop, so identity can tell "not
/// provisioned yet" from success and re-provision first.
pub async fn set_suspended(
    db: &Db,
    tenant_id: &str,
    suspension: Option<(BsonDateTime, &str)>,
) -> AppResult<()> {
    crate::core::logging::domain::tracked("tenants.suspension", async move {
        validate_tenant_key(tenant_id)?;
        if !repository::set_tenant_suspended(db, tenant_id, suspension).await? {
            return Err(AppError::not_found_with_code(
                format!("No shop with id '{tenant_id}'"),
                codes::TENANT_NOT_FOUND,
            ));
        }
        // Dropping the entry makes this instance re-read (and re-count) on the very next request.
        suspension_cache().remove(tenant_id);
        Ok(())
    })
    .await
}
