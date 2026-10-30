use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::Product;
use crate::modules::inventory::model::ProductDocument;
use crate::modules::inventory::repository;

use mongodb::bson::Document;

/// Converts a page of raw `products` documents — as read by the sync
/// module's cursor scan — into the exact `Product` shape `GET /products`
/// returns. The sync delta feed and the REST snapshot feed have to be
/// byte-identical: the client mirrors both into the same local table, so a
/// row that arrives by delta must not be missing the `category`/
/// `subcategory` display names the read path resolves.
///
/// Names are resolved with two collection reads for the whole page (the
/// category/subcategory collections are small reference data) rather than
/// the per-row `resolve_display_names` used by single-document reads.
pub(crate) async fn hydrate_sync_documents(
    db: &Db,
    documents: Vec<Document>,
) -> AppResult<Vec<Product>> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }

    let categories = repository::category::list_categories_with_subcategories(db).await?;

    let mut category_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut subcategory_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for (category, subcategories) in categories {
        category_names.insert(category.key.clone(), category.name);
        for subcategory in subcategories {
            subcategory_names.insert(subcategory.key, subcategory.name);
        }
    }

    let mut products = Vec::with_capacity(documents.len());
    for document in documents {
        let product = bson::deserialize_from_document::<ProductDocument>(document)?;

        // A key that no longer resolves falls back to displaying the key
        // itself — same rule as `resolve_display_names`, so a stale
        // reference degrades identically on both feeds.
        let category_name = category_names
            .get(&product.category_key)
            .cloned()
            .unwrap_or_else(|| product.category_key.clone());
        let subcategory_name = subcategory_names
            .get(&product.subcategory_key)
            .cloned()
            .unwrap_or_else(|| product.subcategory_key.clone());

        products.push(product.into_product(category_name, subcategory_name));
    }

    super::super::stock_batch::attach_price_tiers(db, &mut products).await?;
    Ok(products)
}
