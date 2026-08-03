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

pub(crate) async fn find_product_by_sku(
    db: &Database,
    sku: &str,
) -> AppResult<Option<ProductDocument>> {
    Ok(products(db).find_one(doc! { "sku": sku }).await?)
}

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

pub(crate) async fn delete_products(db: &Database, ids: Vec<ObjectId>) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    Ok(products(db)
        .delete_many(doc! { "_id": { "$in": ids } })
        .await?
        .deleted_count)
}

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

pub(crate) async fn count_products_in_category(db: &Database, category: &str) -> AppResult<u64> {
    Ok(products(db)
        .count_documents(doc! { "category": category })
        .await?)
}

pub(crate) async fn count_products_in_subcategory(
    db: &Database,
    category: &str,
    subcategory: &str,
) -> AppResult<u64> {
    Ok(products(db)
        .count_documents(doc! { "category": category, "subcategory": subcategory })
        .await?)
}

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
