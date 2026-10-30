use super::directory::directory;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::modules::tenants::model::TenantDocument;

/// Inserts a new tenant. The unique `shop_code` index turns a racing duplicate
/// into a driver error, which the caller reports as a conflict.
pub(crate) async fn insert_tenant(db: &Db, document: &TenantDocument) -> AppResult<()> {
    directory(db)?.insert_one(document).await?;
    Ok(())
}
