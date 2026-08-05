// Mongo access for the `subcategories` collection only. Every subcategory
// belongs to exactly one category (`category_key`), but this collection has
// no opinion on whether that category still exists — that's a `service`
// concern (see `service::category`).

use futures_util::TryStreamExt;
use mongodb::{Collection, Database, bson::doc};

use crate::{core::error::AppResult, modules::inventory::model::SubcategoryDocument};

fn subcategories(db: &Database) -> Collection<SubcategoryDocument> {
    db.collection("subcategories")
}

pub(crate) async fn find_subcategory_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    Ok(subcategories(db).find_one(doc! { "key": key }).await?)
}

/// Backs the duplicate-name guard on `service::category::add_subcategory` —
/// names only need to be unique within a single category, not globally.
pub(crate) async fn find_subcategory_by_category_and_name(
    db: &Database,
    category_key: &str,
    name: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    Ok(subcategories(db)
        .find_one(doc! { "category_key": category_key, "name": name })
        .await?)
}

/// Sorted by name for stable, predictable output.
pub(crate) async fn list_subcategories_by_category(
    db: &Database,
    category_key: &str,
) -> AppResult<Vec<SubcategoryDocument>> {
    let mut cursor = subcategories(db)
        .find(doc! { "category_key": category_key })
        .sort(doc! { "name": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Fetches every subcategory across all categories in one query — used by
/// `service::category::list_categories`/`get_valid_categories` to group by
/// `category_key` in memory rather than issuing one query per category.
pub(crate) async fn list_all_subcategories(db: &Database) -> AppResult<Vec<SubcategoryDocument>> {
    let mut cursor = subcategories(db)
        .find(doc! {})
        .sort(doc! { "name": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Fetches every subcategory matching one of `keys` in a single query —
/// used by `service::product::list_products` to batch-resolve display names
/// for a page of products instead of one lookup per product.
pub(crate) async fn find_subcategories_by_keys(
    db: &Database,
    keys: &[String],
) -> AppResult<Vec<SubcategoryDocument>> {
    let mut cursor = subcategories(db)
        .find(doc! { "key": { "$in": keys } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

pub(crate) async fn insert_subcategory(
    db: &Database,
    document: &SubcategoryDocument,
) -> AppResult<()> {
    subcategories(db).insert_one(document).await?;
    Ok(())
}

pub(crate) async fn delete_subcategory_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<SubcategoryDocument>> {
    Ok(subcategories(db)
        .find_one_and_delete(doc! { "key": key })
        .await?)
}

/// Cascade-deletes every subcategory under a category once
/// `service::category::delete_category`'s "still referenced by products"
/// guard has passed — safe because a product can only hold a
/// `subcategory_key` whose owning subcategory has this `category_key`
/// (enforced by `service::product::ensure_valid_category`), so the guard
/// having passed for `category_key` means none of these subcategories are
/// referenced either.
pub(crate) async fn delete_subcategories_by_category(
    db: &Database,
    category_key: &str,
) -> AppResult<u64> {
    Ok(subcategories(db)
        .delete_many(doc! { "category_key": category_key })
        .await?
        .deleted_count)
}
