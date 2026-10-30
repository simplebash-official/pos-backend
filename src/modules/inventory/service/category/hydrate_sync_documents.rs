use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::CategoryInfo;
use crate::modules::inventory::model::{CategoryDocument, SubcategoryDocument};
use crate::modules::inventory::repository;

/// Converts a page of raw `categories` documents — as read by the sync
/// module's cursor scan — into the `CategoryInfo` shape `GET /categories`
/// returns, subcategories included.
///
/// Subcategories live in their own collection, so a raw category document
/// has no `subcategories` field at all. Shipping it unhydrated would let a
/// delta pull overwrite a good mirrored row with one whose subcategory list
/// is `undefined`, which is why the sync feed resolves them here rather
/// than exposing `subcategories` as its own syncable resource.
pub(crate) async fn hydrate_sync_documents(
    db: &Db,
    documents: Vec<mongodb::bson::Document>,
) -> AppResult<Vec<CategoryInfo>> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }

    let mut subcategories_by_category: std::collections::HashMap<String, Vec<SubcategoryDocument>> =
        std::collections::HashMap::new();
    for (category, subcategories) in
        repository::category::list_categories_with_subcategories(db).await?
    {
        subcategories_by_category.insert(category.key, subcategories);
    }

    let mut categories = Vec::with_capacity(documents.len());
    for document in documents {
        let category = bson::deserialize_from_document::<CategoryDocument>(document)?;
        let subcategories = subcategories_by_category
            .remove(&category.key)
            .unwrap_or_default()
            .into_iter()
            .map(SubcategoryDocument::into_subcategory_info)
            .collect();
        categories.push(category.into_category_info(subcategories));
    }

    Ok(categories)
}
