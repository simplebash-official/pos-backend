use super::normalize_shop_code::normalize_shop_code;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::core::tenancy::Tenant;

use crate::modules::tenants::repository;

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
