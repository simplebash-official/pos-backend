// Mongo access for the `purchases` collection only. Functions here never
// interpret a missing document as an error — they return `Vec`/counts
// straight from the driver and leave the "not found" -> `AppError`
// translation to `service`. Visibility is `pub(crate)` so `service` can
// call in, but the `mod repository;` declaration in `purchases/mod.rs` is
// private, so none of this is reachable from outside this module tree.

use futures_util::TryStreamExt;
use mongodb::{Collection, Database, bson::doc};

use crate::{core::error::AppResult, modules::purchases::model::PurchaseDocument};

fn purchases(db: &Database) -> Collection<PurchaseDocument> {
    db.collection("purchases")
}

/// Inserts `document` and backfills its `id` from the driver-generated
/// `_id`, since the caller builds the document with `id: None`.
pub(crate) async fn insert_purchase(
    db: &Database,
    mut document: PurchaseDocument,
) -> AppResult<PurchaseDocument> {
    let result = purchases(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// Sorted `date` descending — "newest first" per the frontend contract.
#[allow(dead_code)]
pub(crate) async fn list_purchases_by_supplier(
    db: &Database,
    supplier_key: &str,
) -> AppResult<Vec<PurchaseDocument>> {
    let mut cursor = purchases(db)
        .find(doc! { "supplier_key": supplier_key })
        .sort(doc! { "date": -1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

#[allow(dead_code)]
pub(crate) async fn list_purchases_by_product(
    db: &Database,
    product_key: &str,
) -> AppResult<Vec<PurchaseDocument>> {
    let mut cursor = purchases(db)
        .find(doc! { "product_key": product_key })
        .sort(doc! { "date": -1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Backs the `SUPPLIER_HAS_PURCHASES` delete guard in
/// `suppliers::service::delete_supplier`.
pub(crate) async fn count_purchases_for_supplier(
    db: &Database,
    supplier_key: &str,
) -> AppResult<u64> {
    Ok(purchases(db)
        .count_documents(doc! { "supplier_key": supplier_key, "deleted_at": { "$exists": false } })
        .await?)
}

pub(crate) async fn list_purchases_paginated(
    db: &Database,
    filter: mongodb::bson::Document,
    skip: u64,
    limit: i64,
) -> AppResult<(Vec<PurchaseDocument>, u64)> {
    let collection = purchases(db);
    let total = collection.count_documents(filter.clone()).await?;

    let mut cursor = collection
        .find(filter)
        .sort(doc! { "date": -1 })
        .skip(skip)
        .limit(limit)
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok((items, total))
}
