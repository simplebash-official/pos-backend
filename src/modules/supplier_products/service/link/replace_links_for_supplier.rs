use crate::clients::db::Db;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;

use crate::domain::supplier_products::{BulkReplaceLinksRequest, SupplierProductLink};
use crate::modules::inventory::service::product as inventory_product;
use crate::modules::supplier_products::model::SupplierProductLinkDocument;
use crate::modules::supplier_products::repository;
use crate::modules::suppliers::service as supplier_service;
use mongodb::bson::DateTime as BsonDateTime;
use std::collections::HashMap;

/// Replaces every link for a supplier with the given set of `productKeys`:
/// validates the supplier and every product key exist, then deletes all of
/// the supplier's existing links and re-inserts the new set — preserving
/// `costPriceCents`/`notes` for any product key that already had a link.
pub(crate) async fn replace_links_for_supplier(
    db: &Db,
    supplier_key: String,
    body: BulkReplaceLinksRequest,
) -> AppResult<Vec<SupplierProductLink>> {
    supplier_service::get_supplier_by_key(db, &supplier_key).await?;
    for product_key in &body.product_keys {
        inventory_product::get_product_by_key(db, product_key).await?;
    }

    let existing_by_product: HashMap<String, SupplierProductLinkDocument> =
        repository::list_links_by_supplier(db, &supplier_key)
            .await?
            .into_iter()
            .map(|link| (link.product_key.clone(), link))
            .collect();

    repository::delete_links_by_supplier(db, &supplier_key).await?;

    let now = BsonDateTime::now();
    let mut links = Vec::with_capacity(body.product_keys.len());
    for product_key in body.product_keys {
        let document = match existing_by_product.get(&product_key) {
            Some(existing) => SupplierProductLinkDocument {
                id: None,
                key: existing.key.clone(),
                supplier_key: supplier_key.clone(),
                product_key,
                cost_price_cents: existing.cost_price_cents,
                notes: existing.notes.clone(),
                version: existing.version + 1,
                created_at: existing.created_at,
                updated_at: now,
                deleted_at: None,
                updated_by_device: None,
            },
            None => SupplierProductLinkDocument {
                id: None,
                key: generate_id(prefixes::SUPPLIER_PRODUCT),
                supplier_key: supplier_key.clone(),
                product_key,
                cost_price_cents: None,
                notes: None,
                version: 1,
                created_at: now,
                updated_at: now,
                deleted_at: None,
                updated_by_device: None,
            },
        };
        let inserted = repository::insert_link(db, document).await?;
        links.push(inserted.into_link());
    }

    Ok(links)
}
