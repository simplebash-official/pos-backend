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

use crate::{core::error::AppResult, modules::inventory::model::CategoryDocument};

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

/// Fetches every category matching one of `keys` in a single query — used
/// by `service::product::list_products` to batch-resolve display names for
/// a page of products instead of one lookup per product.
pub(crate) async fn find_categories_by_keys(
    db: &Database,
    keys: &[String],
) -> AppResult<Vec<CategoryDocument>> {
    let mut cursor = categories(db).find(doc! { "key": { "$in": keys } }).await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Sorted by name for stable, predictable `GET /categories` output.
pub(crate) async fn list_categories(db: &Database) -> AppResult<Vec<CategoryDocument>> {
    let mut cursor = categories(db)
        .find(doc! {})
        .sort(doc! { "name": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
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
