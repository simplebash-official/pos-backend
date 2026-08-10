// Mongo access for the `supplier_products` collection only. Functions here
// never interpret a missing document as an error — they return
// `Option`/`Vec`/counts straight from the driver and leave the "not found"
// -> `AppError` translation to `service`. Visibility is `pub(crate)` so
// `service` can call in, but the `mod repository;` declaration in
// `supplier_products/mod.rs` is private, so none of this is reachable from
// outside this module tree.

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc},
    options::ReturnDocument,
};

use crate::{
    core::error::AppResult, modules::supplier_products::model::SupplierProductLinkDocument,
};

fn supplier_products(db: &Database) -> Collection<SupplierProductLinkDocument> {
    db.collection("supplier_products")
}

pub(crate) async fn find_link(
    db: &Database,
    supplier_key: &str,
    product_key: &str,
) -> AppResult<Option<SupplierProductLinkDocument>> {
    Ok(supplier_products(db)
        .find_one(doc! { "supplier_key": supplier_key, "product_key": product_key })
        .await?)
}

/// Sorted by `created_at` so `GET /supplier-products?supplierKey=` reads in
/// the order products were linked.
pub(crate) async fn list_links_by_supplier(
    db: &Database,
    supplier_key: &str,
) -> AppResult<Vec<SupplierProductLinkDocument>> {
    let mut cursor = supplier_products(db)
        .find(doc! { "supplier_key": supplier_key })
        .sort(doc! { "created_at": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

#[allow(dead_code)]
pub(crate) async fn list_links_by_product(
    db: &Database,
    product_key: &str,
) -> AppResult<Vec<SupplierProductLinkDocument>> {
    let mut cursor = supplier_products(db)
        .find(doc! { "product_key": product_key })
        .sort(doc! { "created_at": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Inserts `document` and backfills its `id` from the driver-generated
/// `_id`, since the caller builds the document with `id: None`.
pub(crate) async fn insert_link(
    db: &Database,
    mut document: SupplierProductLinkDocument,
) -> AppResult<SupplierProductLinkDocument> {
    let result = supplier_products(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// `set_doc` is built by the caller (`service::link::upsert_link`), which
/// decides which fields actually change — this applies it, bumps `version`,
/// and clears any `deleted_at` tombstone. The filter deliberately omits a
/// `deleted_at` condition (as does `find_link`) so an upsert revives a
/// soft-deleted link; the revive must `$unset` rather than `$set` null,
/// because every read filters on `deleted_at: { "$exists": false }`, which a
/// present-but-null field fails. `$unset` on an absent field is a no-op, so
/// the ordinary update path is unaffected.
pub(crate) async fn update_link(
    db: &Database,
    supplier_key: &str,
    product_key: &str,
    set_doc: Document,
) -> AppResult<Option<SupplierProductLinkDocument>> {
    Ok(supplier_products(db)
        .find_one_and_update(
            doc! { "supplier_key": supplier_key, "product_key": product_key },
            doc! {
                "$set": set_doc,
                "$inc": { "version": 1 },
                "$unset": { "deleted_at": "" },
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_link(
    db: &Database,
    supplier_key: &str,
    product_key: &str,
) -> AppResult<Option<SupplierProductLinkDocument>> {
    Ok(supplier_products(db)
        .find_one_and_delete(doc! { "supplier_key": supplier_key, "product_key": product_key })
        .await?)
}

/// Backs the cascade-delete when a supplier is removed
/// (`suppliers::service::delete_supplier`).
pub(crate) async fn delete_links_by_supplier(db: &Database, supplier_key: &str) -> AppResult<u64> {
    Ok(supplier_products(db)
        .delete_many(doc! { "supplier_key": supplier_key })
        .await?
        .deleted_count)
}

/// Backs the cascade-delete when a product is removed
/// (`inventory::service::product::delete_product`/`delete_products`).
pub(crate) async fn delete_links_by_product(db: &Database, product_key: &str) -> AppResult<u64> {
    Ok(supplier_products(db)
        .delete_many(doc! { "product_key": product_key })
        .await?
        .deleted_count)
}

pub(crate) async fn list_links_paginated(
    db: &Database,
    filter: Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<SupplierProductLinkDocument>, u64)> {
    let collection = supplier_products(db);
    let total = collection.count_documents(filter.clone()).await?;

    let mut cursor = collection
        .find(filter)
        .sort(doc! { "created_at": 1 })
        .skip(skip)
        .limit(limit)
        .await?;

    let mut items = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        items.push(doc);
    }
    Ok((items, total))
}
