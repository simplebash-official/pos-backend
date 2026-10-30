use super::record_link_deletes::record_link_deletes;
use super::supplier_products::supplier_products;
use crate::clients::db::Db;

use crate::clients::tenant_db::TenantDatabase;
use crate::core::error::AppResult;

use futures_util::TryStreamExt;

use mongodb::bson::Document;

/// Removes every link matching `filter`, reporting the removed keys to sync.
pub(super) async fn delete_links_where(
    db: &Db,
    mongo: &TenantDatabase,
    filter: Document,
) -> AppResult<u64> {
    let keys: Vec<String> = supplier_products(mongo)
        .find(filter.clone())
        .await?
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .map(|link| link.key)
        .collect();
    let deleted = supplier_products(mongo)
        .delete_many(filter)
        .await?
        .deleted_count;
    record_link_deletes(db, keys).await?;
    Ok(deleted)
}
