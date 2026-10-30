use super::shop_profile_key::SHOP_PROFILE_KEY;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::settings::ShopProfileResponse;

use crate::modules::settings::repository;
use crate::modules::tenants::repository as tenants_repository;

/// Fetches the shop profile for the active tenant or local installation.
/// If no profile has been saved yet, initializes with clean defaults:
/// `trading_name` set to the tenant's registered store name, and all other fields empty.
pub(crate) async fn get_shop_profile(
    db: &Db,
    tenant_id: Option<&str>,
) -> AppResult<ShopProfileResponse> {
    if let Some(doc) = repository::find_shop_profile(db, SHOP_PROFILE_KEY).await? {
        return Ok(doc.into_response());
    }

    let mut response = ShopProfileResponse::default();

    if let Some(tid) = tenant_id {
        response.trading_name = tenants_repository::find_tenant_by_key(db, tid)
            .await
            .ok()
            .flatten()
            .map(|t| t.name)
            .unwrap_or_default();
    }

    Ok(response)
}
