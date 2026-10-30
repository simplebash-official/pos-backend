use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::modules::tenants::model::TenantDocument;

pub(super) fn directory(
    db: &Db,
) -> AppResult<crate::clients::tenant_db::ScopedCollection<TenantDocument>> {
    let mongo = db.as_mongo().ok_or_else(|| {
        AppError::internal("the tenant directory requires MongoDB (TENANT_MODE=multi)")
    })?;
    Ok(mongo.platform_collection::<TenantDocument>("tenants"))
}
