// Business rules for the tenant directory: shop-code format, creation, and
// resolving a code to the `Tenant` scope a login runs in.

use mongodb::bson::DateTime as BsonDateTime;

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
        tenancy::Tenant,
    },
    modules::tenants::{model::TenantDocument, repository},
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
        if !key.starts_with("tnt_") || key.len() < 8 || key.chars().any(char::is_whitespace) {
            return Err(AppError::validation(
                "Tenant id must look like tnt_... (copy it from the identity service)",
            ));
        }
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
        assert_eq!(normalize_shop_code("  Acme-Repairs ").unwrap(), "acme-repairs");
        for bad in ["ab", "-abc", "abc-", "has space", "under_score", "", &"x".repeat(33)] {
            assert!(normalize_shop_code(bad).is_err(), "{bad:?}");
        }
    }
}
