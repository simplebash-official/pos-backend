use super::compact_tenant::compact_tenant;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::core::tenancy::{Tenant, with_tenant};
use crate::modules::sync::cloud_store;

use mongodb::bson::{Document, doc};

/// Compacts every tenant that has sync state. Called daily by the consumer.
pub async fn compact_all(db: &Db, retention_days: i64) -> AppResult<u64> {
    let Some(mongo) = db.as_mongo() else {
        return Ok(0);
    };
    let tenants = mongo
        .unscoped()
        .collection::<Document>(cloud_store::META)
        .distinct("tenant_id", doc! {})
        .await?;
    let mut total = 0;
    for tenant in tenants {
        let Some(id) = tenant.as_str() else { continue };
        let Ok(scope) = Tenant::id(id) else { continue };
        total += with_tenant(scope, compact_tenant(db, retention_days)).await?;
    }
    Ok(total)
}
