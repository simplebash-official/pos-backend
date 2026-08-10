// Mongo access for the `suppliers` collection only. Functions here never
// interpret a missing document as an error — they return `Option`/`Vec`/
// counts straight from the driver and leave the "not found" -> `AppError`
// translation to `service`. Visibility is `pub(crate)` so `service` can call
// in, but the `mod repository;` declaration in `suppliers/mod.rs` is
// private, so none of this is reachable from outside the `suppliers` module
// tree (see `modules::inventory::repository` for the same pattern).

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{Document, doc, oid::ObjectId},
    options::ReturnDocument,
};

use crate::{core::error::AppResult, modules::suppliers::model::SupplierDocument};

fn suppliers(db: &Database) -> Collection<SupplierDocument> {
    db.collection("suppliers")
}

pub(crate) async fn find_supplier_by_id(
    db: &Database,
    id: ObjectId,
) -> AppResult<Option<SupplierDocument>> {
    Ok(suppliers(db)
        .find_one(doc! { "_id": id, "deleted_at": { "$exists": false } })
        .await?)
}

/// Looked up by `key` rather than `_id` — the entry point `supplier_products`/
/// `purchases` use to validate a `supplierKey` and enrich their responses.
pub(crate) async fn find_supplier_by_key(
    db: &Database,
    key: &str,
) -> AppResult<Option<SupplierDocument>> {
    Ok(suppliers(db)
        .find_one(doc! { "key": key, "deleted_at": { "$exists": false } })
        .await?)
}

/// Fetches every supplier matching one of `keys` in a single query — used
/// by `service::get_suppliers_by_keys` (the cross-module batch lookup
/// `purchases` uses to enrich a page of purchase history with supplier
/// display data instead of one lookup per row).
pub(crate) async fn find_suppliers_by_keys(
    db: &Database,
    keys: &[String],
) -> AppResult<Vec<SupplierDocument>> {
    let mut cursor = suppliers(db)
        .find(doc! { "key": { "$in": keys }, "deleted_at": { "$exists": false } })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

pub(crate) async fn insert_supplier(
    db: &Database,
    mut document: SupplierDocument,
) -> AppResult<SupplierDocument> {
    let result = suppliers(db).insert_one(&document).await?;
    document.id = Some(
        result
            .inserted_id
            .as_object_id()
            .expect("inserted_id is always an ObjectId for an auto-generated _id"),
    );
    Ok(document)
}

/// `set_doc` is built by the caller (`service::create_supplier`/`update_supplier`),
/// which decides which fields actually change — this just applies it.
pub(crate) async fn update_supplier(
    db: &Database,
    id: ObjectId,
    set_doc: Document,
) -> AppResult<Option<SupplierDocument>> {
    Ok(suppliers(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! { "$set": set_doc, "$inc": { "version": 1 } },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

pub(crate) async fn delete_supplier(
    db: &Database,
    id: ObjectId,
    device_id: Option<String>,
) -> AppResult<Option<SupplierDocument>> {
    let now = mongodb::bson::DateTime::now();
    let mut set_doc = doc! {
        "deleted_at": now,
        "updated_at": now,
    };
    if let Some(device) = device_id {
        set_doc.insert("updated_by_device", device);
    }
    Ok(suppliers(db)
        .find_one_and_update(
            doc! { "_id": id, "deleted_at": { "$exists": false } },
            doc! {
                "$set": set_doc,
                "$inc": { "version": 1 }
            },
        )
        .return_document(ReturnDocument::After)
        .await?)
}

/// Sorted by name for stable, predictable `GET /suppliers` output.
/// `filter` is fully assembled by the caller (`service::list_suppliers`).
pub(crate) async fn list_suppliers(
    db: &Database,
    filter: Document,
) -> AppResult<Vec<SupplierDocument>> {
    let mut effective_filter = filter;
    if !effective_filter.contains_key("deleted_at") {
        effective_filter.insert("deleted_at", doc! { "$exists": false });
    }
    let mut cursor = suppliers(db)
        .find(effective_filter)
        .sort(doc! { "name": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Every distinct value across all suppliers' `supplied_categories` arrays —
/// backs `GET /suppliers/categories`.
pub(crate) async fn distinct_supplied_categories(db: &Database) -> AppResult<Vec<String>> {
    let values = suppliers(db)
        .distinct("supplied_categories", doc! {})
        .await?;

    Ok(values
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect())
}
