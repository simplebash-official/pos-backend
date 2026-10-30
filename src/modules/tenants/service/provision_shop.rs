use super::create_tenant_with_key::create_tenant_with_key;
use super::normalize_shop_code::normalize_shop_code;
use super::provision_outcome::ProvisionOutcome;
use super::provision_owner::ProvisionOwner;
use super::validate_tenant_key::validate_tenant_key;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::core::tenancy::{Tenant, with_tenant};

use crate::modules::tenants::repository;
use crate::modules::{cash_categories, users};

/// Registers a shop the identity service just created and, when `owner` is given,
/// its first Admin login. Idempotent so identity (or an operator) can safely
/// replay it: the same tenant id + shop code only refreshes the shop name, a shop code already
/// owned by a *different* tenant is a 409, and an existing Admin is left alone.
pub async fn provision_shop(
    db: &Db,
    tenant_id: &str,
    shop_code: &str,
    name: &str,
    owner: Option<ProvisionOwner>,
) -> AppResult<ProvisionOutcome> {
    // Validate everything before writing anything, so a bad request leaves no
    // half-registered shop behind.
    validate_tenant_key(tenant_id)?;
    let shop_code = normalize_shop_code(shop_code)?;
    if name.trim().is_empty() {
        return Err(AppError::validation("Tenant name is required"));
    }
    if let Some(owner) = &owner {
        users::service::validate_owner_login(&owner.name, &owner.username, &owner.password_hash)?;
    }

    let tenant_created = match repository::find_tenant_by_shop_code(db, &shop_code).await? {
        Some(existing) if existing.key == tenant_id => {
            // A replay is also how identity pushes a renamed shop.
            if existing.name != name.trim() {
                repository::update_tenant_name(db, tenant_id, name.trim()).await?;
            }
            false
        }
        Some(_) => {
            return Err(AppError::conflict(
                codes::SHOP_CODE_ALREADY_EXISTS,
                format!("Shop code '{shop_code}' is already in use"),
            ));
        }
        None => {
            if repository::find_tenant_by_key(db, tenant_id)
                .await?
                .is_some()
            {
                return Err(AppError::conflict(
                    codes::TENANT_ALREADY_EXISTS,
                    "This tenant is already registered under a different shop code",
                ));
            }
            create_tenant_with_key(db, tenant_id.to_string(), &shop_code, name).await?;
            true
        }
    };

    // Every shop starts with the drawer's built-in cash categories (idempotent on a replay).
    with_tenant(Tenant::id(tenant_id)?, async {
        cash_categories::service::ensure_system_categories(db).await
    })
    .await?;

    let admin_created = match owner {
        None => false,
        Some(owner) => {
            // Users are tenant-owned data: the Admin must be written inside the
            // new shop's scope, not the platform scope this call runs in.
            let scope = Tenant::id(tenant_id)?;
            with_tenant(scope, async {
                users::service::create_owner_admin_if_absent(
                    db,
                    &owner.name,
                    &owner.username,
                    &owner.password_hash,
                )
                .await
            })
            .await?
        }
    };

    Ok(ProvisionOutcome {
        tenant_created,
        admin_created,
    })
}
