// Persistence for the platform `tenants` directory. Mongo only: the directory
// exists solely for multi-tenant deployments. No `AppError`s here beyond the
// engine check - not-found/duplicate decisions belong to `service`.

use mongodb::bson::doc;

use crate::{
    clients::db::Db,
    core::error::{AppError, AppResult},
    modules::tenants::model::TenantDocument,
};

fn directory(db: &Db) -> AppResult<crate::clients::tenant_db::ScopedCollection<TenantDocument>> {
    let mongo = db.as_mongo().ok_or_else(|| {
        AppError::internal("the tenant directory requires MongoDB (TENANT_MODE=multi)")
    })?;
    Ok(mongo.platform_collection::<TenantDocument>("tenants"))
}

/// Finds a tenant by its (already normalised) shop code.
pub(crate) async fn find_tenant_by_shop_code(
    db: &Db,
    shop_code: &str,
) -> AppResult<Option<TenantDocument>> {
    Ok(directory(db)?
        .find_one(doc! { "shop_code": shop_code })
        .await?)
}

/// Inserts a new tenant. The unique `shop_code` index turns a racing duplicate
/// into a driver error, which the caller reports as a conflict.
pub(crate) async fn insert_tenant(db: &Db, document: &TenantDocument) -> AppResult<()> {
    directory(db)?.insert_one(document).await?;
    Ok(())
}
