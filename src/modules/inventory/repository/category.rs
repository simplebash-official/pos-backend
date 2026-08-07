// Mongo access for the `categories` collection only. This collection is
// the single source of truth for both `GET /categories`-style reads and
// (together with `repository::subcategory`) product validation
// (`service::product::ensure_valid_category`) — there is no separate
// hardcoded category list anywhere else in the codebase.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc},
    options::ReturnDocument,
};
use serde::Deserialize;

use crate::{
    core::error::AppResult,
    modules::inventory::model::{CategoryDocument, SubcategoryDocument},
};

fn categories(db: &Database) -> Collection<CategoryDocument> {
    db.collection("categories")
}

/// Only used for the create-time name-uniqueness check
/// (`service::category::create_category`/`update_category`) — every other
/// lookup goes by `key` (see `find_category_by_key`).
pub(crate) async fn find_category_by_name(
    db: &Database,
    name: &str,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db).find_one(doc! { "name": name }).await?)
}

pub(crate) async fn find_category_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db).find_one(doc! { "key": key }).await?)
}

#[derive(Debug, Deserialize)]
struct CategoryWithSubcategories {
    #[serde(flatten)]
    category: CategoryDocument,
    #[serde(default)]
    subcategories: Vec<SubcategoryDocument>,
}

/// Runs a `$lookup` aggregation pipeline joining `categories` to their
/// `subcategories` (on `key`/`category_key`) in a single round trip —
/// replaces the "fetch both collections separately, group subcategories by
/// `category_key` in a `HashMap`" pattern that used to be duplicated across
/// `service::category::list_categories`/`get_valid_categories` and
/// `service::overview::get_inventory_overview`. Subcategories are sorted by
/// name in Rust after the fact (cheap — a handful of items per category)
/// rather than via `$sortArray`, to avoid depending on MongoDB 5.2+.
pub(crate) async fn list_categories_with_subcategories(
    db: &Database,
) -> AppResult<Vec<(CategoryDocument, Vec<SubcategoryDocument>)>> {
    let pipeline = vec![
        doc! { "$sort": { "name": 1 } },
        doc! {
            "$lookup": {
                "from": "subcategories",
                "localField": "key",
                "foreignField": "category_key",
                "as": "subcategories",
            }
        },
    ];

    let mut cursor = categories(db).aggregate(pipeline).await?;
    let mut items = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        let mut item = bson::deserialize_from_document::<CategoryWithSubcategories>(doc)?;
        item.subcategories.sort_by(|a, b| a.name.cmp(&b.name));
        items.push((item.category, item.subcategories));
    }
    Ok(items)
}

pub(crate) async fn insert_category(db: &Database, document: &CategoryDocument) -> AppResult<()> {
    categories(db).insert_one(document).await?;
    Ok(())
}

/// `set_doc` is assembled by the caller (`service::category::update_category`);
/// this just applies it, filtered by the category's immutable `key` rather
/// than its (renameable) `name`.
pub(crate) async fn update_category_fields(
    db: &Database,
    key: &str,
    set_doc: Document,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db)
        .find_one_and_update(doc! { "key": key }, doc! { "$set": set_doc })
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_category(
    db: &Database,
    key: &str,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db)
        .find_one_and_delete(doc! { "key": key })
        .await?)
}

pub(crate) async fn find_category_keys_by_name_pattern(
    db: &Database,
    pattern: &mongodb::bson::Regex,
) -> AppResult<Vec<String>> {
    let mut cursor = categories(db)
        .find(doc! { "name": { "$regex": pattern } })
        .await?;
    let mut keys = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        keys.push(doc.key);
    }
    Ok(keys)
}
