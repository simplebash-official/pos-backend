use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::auth::ShopLookupResponse;
use crate::modules::tenants::service as tenants_service;

/// Looks up a shop by its code and returns its display name (for branding the login screen).
pub(crate) async fn lookup_shop(db: &Db, code: &str) -> AppResult<ShopLookupResponse> {
    let (_, name) = tenants_service::lookup_shop_details(db, code)
        .await?
        .ok_or_else(|| AppError::not_found(format!("No shop found with code '{}'", code.trim())))?;
    Ok(ShopLookupResponse {
        shop_code: code.trim().to_ascii_lowercase(),
        name,
    })
}
