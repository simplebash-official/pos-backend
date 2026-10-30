use super::resolve_display_names::resolve_display_names;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::Product;

use crate::modules::inventory::repository;

use mongodb::bson::oid::ObjectId;

/// Deletes and returns the deleted document (so the handler can echo back
/// what was removed), 404ing if it never existed. Also cascades the delete
/// onto any `supplier_products` links pointing at this product's `key` —
/// mirrors the category/subcategory cascade in `service::category`, just
/// across a module boundary rather than within one.
pub(crate) async fn delete_product(
    db: &Db,
    id: ObjectId,
    expected_version: Option<i64>,
    device_id: Option<String>,
) -> AppResult<Product> {
    crate::core::logging::domain::tracked("inventory.product_deleted", async move {
        let existing = repository::product::find_product_by_id(db, id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;

        if let Some(expected) = expected_version
            && existing.version != expected
        {
            let (category_name, subcategory_name) =
                resolve_display_names(db, &existing.category_key, &existing.subcategory_key)
                    .await?;
            let server_doc = super::super::stock_batch::with_price_tiers(
                db,
                existing
                    .clone()
                    .into_product(category_name, subcategory_name),
            )
            .await?;

            return Err(AppError::conflict_with_details(
                codes::VERSION_CONFLICT,
                "This product was changed on another device.",
                serde_json::json!({
                    "expectedVersion": expected,
                    "serverVersion": existing.version,
                    "updatedByDevice": existing.updated_by_device,
                    "server": server_doc,
                    "conflictingFields": Vec::<String>::new(),
                }),
            ));
        }

        let deleted = repository::product::delete_product(db, id, device_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("Product not found", codes::PRODUCT_NOT_FOUND)
            })?;

        crate::modules::supplier_products::service::link::delete_links_for_product(
            db,
            &deleted.key,
        )
        .await?;

        let (category_name, subcategory_name) =
            resolve_display_names(db, &deleted.category_key, &deleted.subcategory_key).await?;

        super::super::stock_batch::with_price_tiers(
            db,
            deleted.into_product(category_name, subcategory_name),
        )
        .await
    })
    .await
}
