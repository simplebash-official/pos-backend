// Mongo access for the `products` collection only. No validation, no
// `AppError::not_found` — see `repository/mod.rs` for the contract every
// function here follows.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, modules::inventory::model::ProductDocument};

fn products(db: &Database) -> Collection<ProductDocument> {
    db.collection("products")
}

pub(crate) async fn find_product_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db).find_one(doc! { "_id": id }).await?)
}

/// Used for the create-time uniqueness check (`service::product::create_product`) —
/// SKUs must be unique across all products.
pub(crate) async fn find_product_by_sku(
    db: &Database,
    sku: &str,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db).find_one(doc! { "sku": sku }).await?)
}

/// Inserts `document` and backfills its `id` from the driver-generated
/// `_id`, since the caller builds the document with `id: None` (Mongo
/// assigns the `ObjectId` on insert, not before).
pub(crate) async fn insert_product(
    db: &Database,
    mut document: ProductDocument,
) -> AppResult<ProductDocument> {
    let result = products(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// `set_doc` is built by the caller (`service::product::update_product`),
/// which decides which fields actually change — this just applies it.
pub(crate) async fn update_product(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db)
        .find_one_and_update(doc! { "_id": id }, doc! { "$set": set_doc })
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_product(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db).find_one_and_delete(doc! { "_id": id }).await?)
}

/// Batch delete for `DELETE /products`. Guards the empty-list case
/// explicitly — an empty `$in` array is valid Mongo but there's no reason
/// to round-trip to the database for a no-op delete.
pub(crate) async fn delete_products(db: &Database, ids: Vec<ObjectId>) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    Ok(products(db)
        .delete_many(doc! { "_id": { "$in": ids } })
        .await?
        .deleted_count)
}

/// Runs `filter` twice — once to count, once to fetch the page — since
/// Mongo has no single-query "give me the page and the total" operation.
/// `filter`/`sort` are fully assembled by the caller
/// (`service::product::list_products`); this function only executes them.
pub(crate) async fn list_products(
    db: &Database,
    filter: Document,
    sort: Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<ProductDocument>, u64)> {
    let collection = products(db);
    let total = collection.count_documents(filter.clone()).await?;

    let mut cursor = collection
        .find(filter)
        .sort(sort)
        .skip(skip)
        .limit(limit)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }

    Ok((items, total))
}

/// `$expr`/`$lte` compares two fields of the *same* document
/// (`stock_quantity` vs `min_stock_threshold`) — a plain field-to-value
/// filter can't express that, so this needs the aggregation-style `$expr`
/// operator even though it's a simple `find`, not an aggregation pipeline.
pub(crate) async fn find_low_stock_products(db: &Database) -> AppResult<Vec<ProductDocument>> {
    let mut cursor = products(db)
        .find(doc! { "$expr": { "$lte": ["$stock_quantity", "$min_stock_threshold"] } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Sets an already-computed `new_quantity` — the delta math and the
/// negative-stock guard both happen in `service::stock::adjust_stock`
/// before this is called; this function has no opinion on whether the
/// resulting quantity makes sense.
pub(crate) async fn adjust_product_stock(
    db: &Database,
    id: ObjectId,
    new_quantity: i64,
    now: BsonDateTime,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db)
        .find_one_and_update(
            doc! { "_id": id },
            doc! {
                "$set": {
                    "stock_quantity": new_quantity,
                    "updated_at": now,
                }
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

/// Backs the "category still in use" 409 guard on category delete
/// (`service::category::delete_category`) — a category can't be removed
/// while products still reference it by name.
pub(crate) async fn count_products_in_category(db: &Database, category: &str) -> AppResult<u64> {
    Ok(products(db)
        .count_documents(doc! { "category": category })
        .await?)
}

/// Same guard as `count_products_in_category`, scoped to a single
/// subcategory — used before removing a subcategory from a category.
pub(crate) async fn count_products_in_subcategory(
    db: &Database,
    category: &str,
    subcategory: &str,
) -> AppResult<u64> {
    Ok(products(db)
        .count_documents(doc! { "category": category, "subcategory": subcategory })
        .await?)
}

/// Cascades a category rename onto every product that referenced the old
/// name. Products store `category` as a plain name (not a foreign key), so
/// without this, a rename would silently orphan every product that used
/// to point at the old name — see `service::category::update_category`.
pub(crate) async fn rename_products_category(
    db: &Database,
    old_name: &str,
    new_name: &str,
) -> AppResult<u64> {
    Ok(products(db)
        .update_many(
            doc! { "category": old_name },
            doc! { "$set": { "category": new_name } },
        )
        .await?
        .modified_count)
}
