// Mongo access for the `product_serials` collection only — one document
// per physical serialized unit. Never updated except by
// `service::product_serial`'s status-transition helpers; deletion is not
// supported (a unit's history is kept forever, same rationale as
// `stock_movements`).

use futures_util::TryStreamExt;
use mongodb::{
    Collection, Database,
    bson::{doc, oid::ObjectId},
};

use crate::{core::error::AppResult, modules::inventory::model::ProductSerialDocument};

fn product_serials(db: &Database) -> Collection<ProductSerialDocument> {
    db.collection("product_serials")
}

pub(crate) async fn insert_product_serial(
    db: &Database,
    serial: ProductSerialDocument,
) -> AppResult<ProductSerialDocument> {
    let result = product_serials(db).insert_one(&serial).await?;
    let mut inserted = serial;
    inserted.id = result.inserted_id.as_object_id();
    Ok(inserted)
}

pub(crate) async fn find_serial_by_number(
    db: &Database,
    serial_number: &str,
) -> AppResult<Option<ProductSerialDocument>> {
    Ok(product_serials(db)
        .find_one(doc! { "serial_number": serial_number })
        .await?)
}

/// Looks up a serial by number, additionally scoped to a specific product —
/// used when validating a sale/return references a serial that actually
/// belongs to the product line it's attached to.
pub(crate) async fn find_serial_by_number_and_product(
    db: &Database,
    serial_number: &str,
    product_key: &str,
) -> AppResult<Option<ProductSerialDocument>> {
    Ok(product_serials(db)
        .find_one(doc! { "serial_number": serial_number, "product_key": product_key })
        .await?)
}

pub(crate) async fn find_serials_for_product(
    db: &Database,
    product_key: &str,
    status: Option<&str>,
) -> AppResult<Vec<ProductSerialDocument>> {
    let mut filter = doc! { "product_key": product_key };
    if let Some(status) = status {
        filter.insert("status", status);
    }
    let mut cursor = product_serials(db)
        .find(filter)
        .sort(doc! { "serial_number": 1 })
        .await?;

    let mut items = Vec::new();
    while let Some(document) = cursor.try_next().await? {
        items.push(document);
    }
    Ok(items)
}

/// Atomically transitions a serial's status and stamps whichever of
/// `invoice_key`/`sold_at`/`warranty_expires_at`/`credit_note_key` apply to
/// that transition — callers pass only the fields relevant to their
/// transition, everything else in `set_doc` is left untouched.
pub(crate) async fn update_serial(
    db: &Database,
    id: ObjectId,
    set_doc: mongodb::bson::Document,
) -> AppResult<Option<ProductSerialDocument>> {
    let mut update = set_doc;
    update.insert("updated_at", mongodb::bson::DateTime::now());
    Ok(product_serials(db)
        .find_one_and_update(doc! { "_id": id }, doc! { "$set": update })
        .return_document(mongodb::options::ReturnDocument::After)
        .await?)
}
