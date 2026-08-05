// Mongo access for the `categories` collection only. This collection is
// the single source of truth for both `GET /categories`-style reads and
// product validation (`service::product::ensure_valid_category`) — there
// is no separate hardcoded category list anywhere else in the codebase.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, modules::inventory::model::CategoryDocument};

fn categories(db: &Database) -> Collection<CategoryDocument> {
    db.collection("categories")
}

pub(crate) async fn find_category_by_name(
    db: &Database,
    name: &str,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db).find_one(doc! { "name": name }).await?)
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

/// `set_doc` is assembled by the caller
/// (`service::category::update_category`); this just applies it. Note this
/// only ever touches `name`/`icon`/`color` — the cascading rename onto
/// `products` is a separate call the service layer makes to
/// `repository::product::rename_products_category`.
pub(crate) async fn update_category_fields(
    db: &Database,
    name: &str,
    set_doc: Document,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db)
        .find_one_and_update(doc! { "name": name }, doc! { "$set": set_doc })
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_category(
    db: &Database,
    name: &str,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db)
        .find_one_and_delete(doc! { "name": name })
        .await?)
}

pub(crate) async fn add_subcategory(
    db: &Database,
    category: &str,
    subcategory: &str,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db)
        .find_one_and_update(
            doc! { "name": category },
            doc! {
                "$push": { "subcategories": subcategory },
                "$set": { "updated_at": BsonDateTime::now() }
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn remove_subcategory(
    db: &Database,
    category: &str,
    subcategory: &str,
) -> AppResult<Option<CategoryDocument>> {
    Ok(categories(db)
        .find_one_and_update(
            doc! { "name": category },
            doc! {
                "$pull": { "subcategories": subcategory },
                "$set": { "updated_at": BsonDateTime::now() }
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

/// Backs `service::product::ensure_valid_category` — a single query that
/// answers "is `subcategory` actually one of `category`'s subcategories"
/// without fetching the whole document just to scan its `subcategories`
/// array in application code.
pub(crate) async fn category_has_subcategory(
    db: &Database,
    category: &str,
    subcategory: &str,
) -> AppResult<bool> {
    Ok(categories(db)
        .find_one(doc! { "name": category, "subcategories": subcategory })
        .await?
        .is_some())
}
