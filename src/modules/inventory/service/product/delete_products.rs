use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::inventory::repository;

use mongodb::bson::oid::ObjectId;

/// Batch delete. Ids that aren't valid `ObjectId`s are silently dropped
/// rather than failing the whole request — a client sending a mixed batch
/// (some stale/malformed ids alongside valid ones) still gets the valid
/// ones deleted instead of an all-or-nothing rejection. Cascades
/// `supplier_products` link cleanup for every product actually deleted,
/// same as the single-delete path.
pub(crate) async fn delete_products(db: &Db, product_ids: Vec<String>) -> AppResult<u64> {
    crate::core::logging::domain::tracked("inventory.products_bulk_deleted", async move {
        let object_ids: Vec<ObjectId> = product_ids
            .iter()
            .filter_map(|id| ObjectId::parse_str(id).ok())
            .collect();

        // Keys are needed for the `supplier_products` cascade below, so they
        // must be read before the delete removes the documents they came from.
        let keys: Vec<String> = repository::product::find_products_by_ids(db, &object_ids)
            .await?
            .into_iter()
            .map(|document| document.key)
            .collect();

        let deleted_count = repository::product::delete_products(db, object_ids).await?;

        for key in keys {
            crate::modules::supplier_products::service::link::delete_links_for_product(db, &key)
                .await?;
        }

        Ok(deleted_count)
    })
    .await
}
