use super::directory::directory;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::tenants::model::TenantDocument;
use mongodb::bson::doc;

/// Finds a tenant by its (already normalised) shop code.
pub(crate) async fn find_tenant_by_shop_code(
    db: &Db,
    shop_code: &str,
) -> AppResult<Option<TenantDocument>> {
    Ok(directory(db)?
        .find_one(doc! { "shop_code": shop_code })
        .await?)
}
