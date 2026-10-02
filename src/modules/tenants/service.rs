// Business rules for the tenant directory: shop-code format, creation, and
// resolving a code to the `Tenant` scope a login runs in.

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        tenancy::{Tenant, with_tenant},
    },
    modules::{
        tenants::{model::TenantDocument, repository},
        users,
    },
};

/// A directory entry as returned to callers (the binary, tests).
#[derive(Debug, Clone, serde::Serialize)]
pub struct TenantInfo {
    pub key: String,
    pub shop_code: String,
    pub name: String,
}

impl From<TenantDocument> for TenantInfo {
    fn from(d: TenantDocument) -> Self {
        TenantInfo {
            key: d.key,
            shop_code: d.shop_code,
            name: d.name,
        }
    }
}

/// Lowercases and validates a shop code: 3-32 chars of `a-z`, `0-9` and `-`,
/// not starting or ending with `-`.
pub fn normalize_shop_code(raw: &str) -> AppResult<String> {
    let code = raw.trim().to_ascii_lowercase();
    let valid_chars = code
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if code.len() < 3
        || code.len() > 32
        || !valid_chars
        || code.starts_with('-')
        || code.ends_with('-')
    {
        return Err(AppError::validation(
            "Shop code must be 3-32 characters: lowercase letters, digits and hyphens, not starting or ending with a hyphen",
        ));
    }
    Ok(code)
}

/// A tenant id must be one the identity service issued: `tnt_...`.
fn validate_tenant_key(key: &str) -> AppResult<()> {
    if !key.starts_with("tnt_") || key.len() < 8 || key.chars().any(char::is_whitespace) {
        return Err(AppError::validation(
            "Tenant id must look like tnt_... (copy it from the identity service)",
        ));
    }
    Ok(())
}

/// Registers a tenant. 409 `SHOP_CODE_ALREADY_EXISTS` if the code is taken.
pub async fn create_tenant(db: &Db, shop_code: &str, name: &str) -> AppResult<TenantInfo> {
    create_tenant_with_key(db, generate_id(prefixes::TENANT), shop_code, name).await
}

/// Registers a tenant under an existing tenant id - the id the identity service
/// issued for the account's shop - so a staff login by shop code lands in the
/// same tenant whose data the devices sync. The id must look like `tnt_...`.
pub async fn create_tenant_with_key(
    db: &Db,
    key: String,
    shop_code: &str,
    name: &str,
) -> AppResult<TenantInfo> {
    crate::core::logging::domain::tracked("tenants.created", async move {
        validate_tenant_key(&key)?;
        let shop_code = normalize_shop_code(shop_code)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::validation("Tenant name is required"));
        }
        let taken = || {
            AppError::conflict(
                codes::SHOP_CODE_ALREADY_EXISTS,
                format!("Shop code '{shop_code}' is already in use"),
            )
        };
        if repository::find_tenant_by_shop_code(db, &shop_code)
            .await?
            .is_some()
        {
            return Err(taken());
        }
        let document = TenantDocument {
            id: None,
            key,
            shop_code: shop_code.clone(),
            name: name.to_string(),
            created_at: BsonDateTime::now(),
            updated_at: BsonDateTime::now(),
            setup_completed: false,
            sample_data_loaded: false,
            setup_completed_at: None,
        };
        // A concurrent creator can pass the check above; the unique index then
        // rejects the second insert.
        if repository::insert_tenant(db, &document).await.is_err()
            && repository::find_tenant_by_shop_code(db, &shop_code)
                .await?
                .is_some()
        {
            return Err(taken());
        }
        Ok(TenantInfo::from(document))
    })
    .await
}

/// The owner login handed over with a shop: identity's email, name and its
/// existing Argon2id hash (see `users::service::create_owner_admin_if_absent`).
#[derive(Debug, Clone)]
pub struct ProvisionOwner {
    pub email: String,
    pub name: String,
    pub password_hash: String,
}

/// What a provisioning call actually changed; both `false` on a pure replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ProvisionOutcome {
    pub tenant_created: bool,
    pub admin_created: bool,
}

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
        users::service::validate_owner_login(&owner.name, &owner.email, &owner.password_hash)?;
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
                    &owner.email,
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

/// The tenant behind a shop code, or `None` if there is none (or the code is
/// malformed - a malformed code cannot exist in the directory).
pub(crate) async fn lookup_shop_code(db: &Db, code: &str) -> AppResult<Option<Tenant>> {
    let Ok(code) = normalize_shop_code(code) else {
        return Ok(None);
    };
    match repository::find_tenant_by_shop_code(db, &code).await? {
        Some(t) => Ok(Some(Tenant::id(&t.key)?)),
        None => Ok(None),
    }
}

/// The tenant and display name behind a shop code, or `None` if there is none.
pub(crate) async fn lookup_shop_details(db: &Db, code: &str) -> AppResult<Option<(Tenant, String)>> {
    let Ok(code) = normalize_shop_code(code) else {
        return Ok(None);
    };
    match repository::find_tenant_by_shop_code(db, &code).await? {
        Some(t) => Ok(Some((Tenant::id(&t.key)?, t.name))),
        None => Ok(None),
    }
}

/// Resolves a shop code to its tenant scope; 404 `TENANT_NOT_FOUND` if unknown.
pub async fn resolve_shop_code(db: &Db, code: &str) -> AppResult<Tenant> {
    lookup_shop_code(db, code).await?.ok_or_else(|| {
        AppError::not_found_with_code(
            format!("No shop with code '{}'", code.trim()),
            codes::TENANT_NOT_FOUND,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_shop_code;

    #[test]
    fn shop_codes_are_lowercased_and_validated() {
        assert_eq!(
            normalize_shop_code("  Acme-Repairs ").unwrap(),
            "acme-repairs"
        );
        for bad in [
            "ab",
            "-abc",
            "abc-",
            "has space",
            "under_score",
            "",
            &"x".repeat(33),
        ] {
            assert!(normalize_shop_code(bad).is_err(), "{bad:?}");
        }
    }
}
