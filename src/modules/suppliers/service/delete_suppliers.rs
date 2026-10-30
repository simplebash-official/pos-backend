use super::delete_supplier::delete_supplier;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::oid::ObjectId;

/// Batch delete for `DELETE /suppliers/batch`. Ids that aren't valid
/// `ObjectId`s, that don't resolve to an existing supplier, or that are
/// blocked by the purchase-history guard are silently skipped rather than
/// failing the whole request — same "valid ones still succeed" semantics as
/// `inventory::service::product::delete_products`.
pub(crate) async fn delete_suppliers(db: &Db, ids: Vec<String>) -> AppResult<u64> {
    crate::core::logging::domain::tracked("suppliers.bulk_deleted", async move {
        let mut deleted_count = 0u64;
        for id in ids {
            if let Ok(object_id) = ObjectId::parse_str(&id)
                && delete_supplier(db, object_id, None, None).await.is_ok()
            {
                deleted_count += 1;
            }
        }
        Ok(deleted_count)
    })
    .await
}
