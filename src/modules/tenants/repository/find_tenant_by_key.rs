use super::directory::directory;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::tenants::model::TenantDocument;
use mongodb::bson::doc;

/// Finds a tenant by its immutable id (`tnt_...`).
pub(crate) async fn find_tenant_by_key(db: &Db, key: &str) -> AppResult<Option<TenantDocument>> {
    Ok(directory(db)?.find_one(doc! { "key": key }).await?)
}
