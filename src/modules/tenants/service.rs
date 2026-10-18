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
        settings,
        tenants::{model::TenantDocument, repository, usage},
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
            suspended_at: None,
            suspended_reason: None,
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

/// The owner login handed over with a shop: the fixed `admin` username, the
/// owner's name and the Argon2id hash of the admin password they chose (see `users::service::create_owner_admin_if_absent`).
#[derive(Debug, Clone)]
pub struct ProvisionOwner {
    pub username: String,
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

/// The tenant behind a shop code, or `None` if there is none (or the code is
/// malformed - a malformed code cannot exist in the directory).
pub(crate) async fn lookup_shop_code(db: &Db, code: &str) -> AppResult<Option<Tenant>> {
    let Ok(code) = normalize_shop_code(code) else {
        return Ok(None);
    };
    if db.as_mongo().is_none() {
        return Ok(None);
    }
    match repository::find_tenant_by_shop_code(db, &code).await? {
        Some(t) => Ok(Some(Tenant::id(&t.key)?)),
        None => Ok(None),
    }
}

/// The tenant and display name behind a shop code, or `None` if there is none.
pub(crate) async fn lookup_shop_details(
    db: &Db,
    code: &str,
) -> AppResult<Option<(Tenant, String)>> {
    let Ok(code) = normalize_shop_code(code) else {
        return Ok(None);
    };
    if db.as_mongo().is_none() {
        return Ok(None);
    }
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

// ----------------------------------------------------------------------------
// Suspension
// ----------------------------------------------------------------------------

/// How long a looked-up suspension flag is trusted before the next request re-reads it. Changes
/// made through `set_suspended` on this instance take effect at once; another instance of the
/// backend picks them up within this window, which is the "about 30 seconds" the cloud promises.
const SUSPENSION_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// What a suspended shop's staff are shown: when and why the account was suspended, and proof that
/// the shop's data is still stored.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SuspensionInfo {
    pub shop_name: String,
    /// RFC 3339.
    pub suspended_at: String,
    pub reason: String,
    pub retained: Retained,
}

/// Counts read from the shop's own data (no amounts, no record content).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Retained {
    pub bills: u64,
    pub products: u64,
    pub customers: u64,
    pub staff: u64,
    /// RFC 3339 time of the newest non-voided invoice.
    pub last_bill_at: Option<String>,
}

type SuspensionCache =
    std::collections::HashMap<String, (Option<SuspensionInfo>, std::time::Instant)>;

static SUSPENDED: std::sync::LazyLock<std::sync::Mutex<SuspensionCache>> =
    std::sync::LazyLock::new(Default::default);

fn suspension_cache() -> std::sync::MutexGuard<'static, SuspensionCache> {
    SUSPENDED.lock().unwrap_or_else(|e| e.into_inner())
}

/// The shop's current suspension, `None` while it is active. Cached for `SUSPENSION_TTL`, so the
/// retained-data counts are computed at most once per window however often a refused client retries.
/// Always `None` outside the multi-tenant (MongoDB) cloud and for a tenant that is not in the
/// directory (its token is then refused elsewhere).
pub(crate) async fn suspension_of(db: &Db, tenant_key: &str) -> AppResult<Option<SuspensionInfo>> {
    if db.as_mongo().is_none() {
        return Ok(None);
    }
    if let Some(known) = suspension_cache()
        .get(tenant_key)
        .filter(|(_, read_at)| read_at.elapsed() < SUSPENSION_TTL)
        .map(|(info, _)| info.clone())
    {
        return Ok(known);
    }
    let info = match repository::find_tenant_by_key(db, tenant_key).await? {
        Some(TenantDocument {
            name,
            suspended_at: Some(at),
            suspended_reason,
            ..
        }) => {
            let reason = suspended_reason.ok_or_else(|| {
                AppError::internal(format!("shop {tenant_key} is suspended without a reason"))
            })?;
            let usage = usage::usage_for(db, tenant_key).await?;
            Some(SuspensionInfo {
                shop_name: name,
                suspended_at: at
                    .try_to_rfc3339_string()
                    .map_err(|e| AppError::internal(e.to_string()))?,
                reason,
                retained: Retained {
                    bills: usage.bills_total,
                    products: usage.products,
                    customers: usage.customers,
                    staff: usage.users,
                    last_bill_at: usage.last_bill_at,
                },
            })
        }
        _ => None,
    };
    suspension_cache().insert(
        tenant_key.to_string(),
        (info.clone(), std::time::Instant::now()),
    );
    Ok(info)
}

/// The refusal every request of a suspended shop gets; `details` feeds the suspended page.
pub(crate) fn suspended_error(info: &SuspensionInfo) -> AppError {
    AppError::custom_with_details(
        axum::http::StatusCode::FORBIDDEN,
        codes::ACCOUNT_SUSPENDED,
        "This shop's cloud access is suspended. The desktop app keeps working offline; contact SimpleBash to restore access.",
        serde_json::to_value(info).expect("SuspensionInfo serialises"),
    )
}

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

/// Runs `work` inside the shop's tenant scope; 404 `TENANT_NOT_FOUND` for a
/// shop that is not in the directory.
async fn in_shop<T, F: std::future::Future<Output = AppResult<T>>>(
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

/// The `(kind, family)` document designs a shop has chosen (operator console).
pub async fn document_selections(db: &Db, tenant_id: &str) -> AppResult<Vec<(String, String)>> {
    in_shop(
        db,
        tenant_id,
        settings::service::list_document_selections(db),
    )
    .await
}

/// Clears the shop's choices of the given design families, and the option
/// values it saved for them; returns the `(kind, family)` choices removed.
pub async fn reset_document_selections(
    db: &Db,
    tenant_id: &str,
    families: &[String],
) -> AppResult<Vec<(String, String)>> {
    in_shop(
        db,
        tenant_id,
        settings::service::reset_document_families(db, families),
    )
    .await
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
